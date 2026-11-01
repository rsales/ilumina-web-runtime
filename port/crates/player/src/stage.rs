//! The score at run time (M2 slice 3): sprite channels, behavior instances
//! and the movie's event order.
//!
//! Director's startup order: `prepareMovie`, then the first frame's sprites
//! get their behaviors and `beginSprite`, then `prepareFrame`, `startMovie`,
//! `enterFrame`, `exitFrame`. Frame events go to every sprite's behaviors,
//! then to the frame script, and to movie scripts only when the frame
//! script does not handle them.

use crate::host::PlayerHost;
use lingo_vm::value::ObjectRef;
use lingo_vm::{LingoError, Value, Vm};

/// One sprite channel's run-time state.
#[derive(Clone, Debug, Default)]
pub struct SpriteState {
    /// (cast lib, member number); `None` = empty channel.
    pub member: Option<(i64, i64)>,
    /// Registration point position on the stage.
    pub loc: (i64, i64),
    pub width: i64,
    pub height: i64,
    pub ink: i64,
    pub blend: i64,
    pub visible: bool,
    pub puppet: bool,
    /// Drawn at `width`×`height` rather than the member's own size.
    pub stretched: bool,
    pub fore_color: i64,
    pub back_color: i64,
    pub loc_z: i64,
    pub cursor: Value,
    /// Behavior instances, in the order the author attached them.
    pub instances: Vec<Value>,
    /// The score's record (colours, kind) the run-time state started from.
    pub score: Option<rifx::score::Sprite>,
    /// Properties of an Xtra sprite (OSControl, TreeView…), as set by Lingo.
    pub xtra_props: Vec<(String, Value)>,
    /// The score interval (first, last frame) this sprite belongs to.
    pub span: Option<(u32, u32)>,
    /// Corners Lingo set with `sprite.quad` (top-left, top-right,
    /// bottom-right, bottom-left); ⏳ drawn as the bounding rect.
    pub quad: Option<[(f64, f64); 4]>,
}

fn objects(list: &[Value]) -> Vec<ObjectRef> {
    list.iter()
        .filter_map(|v| match v {
            Value::Object(o) => Some(o.clone()),
            _ => None,
        })
        .collect()
}

/// `value()` of a score parameter string as (property, value) pairs.
fn params(text: Option<&str>) -> Vec<(String, Value)> {
    match text.map(lingo_vm::builtins::value_of) {
        Some(Value::PropList(l)) => l
            .borrow()
            .iter()
            .map(|(k, v)| (k.to_text(), v.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Behavior instances for an interval's behaviors (`spriteNum` = `n`).
fn behaviors(
    vm: &mut Vm,
    host: &PlayerHost,
    list: &[rifx::score::BehaviorRef],
    n: u16,
) -> Vec<Value> {
    list.iter()
        .filter_map(|b| {
            let script = host
                .member_info(b.cast_lib as i64, b.member as i64)
                .and_then(|m| m.script)?;
            Some(vm.new_behavior(script, &params(b.params.as_deref()), n as i64))
        })
        .collect()
}

/// Move the playhead to `frame` (1-based): sprites whose score interval
/// ends get `endSprite` and leave, sprites whose interval starts get their
/// behaviors and `beginSprite` (sprites staying in the same interval keep
/// what Lingo did to them; puppets stay). Same for the frame script.
/// Returns the sprites that began.
pub fn enter_frame(
    vm: &mut Vm,
    host: &mut PlayerHost,
    frame: usize,
) -> Result<Vec<u16>, LingoError> {
    let Some(score) = host.score.clone() else {
        return Ok(Vec::new());
    };
    let records = score.sprites(frame).unwrap_or_default();
    let span_of = |n: u16| {
        score
            .intervals_at(frame)
            .find(|i| i.sprite == n)
            .map(|i| (i.start, i.end))
    };
    // Leaving: not in this frame, or a different interval / record.
    let mut ending = Vec::new();
    for (&n, st) in &host.sprites {
        if st.puppet {
            continue;
        }
        let rec = records.iter().find(|r| r.number == n);
        let same = match rec {
            None => false,
            Some(r) => match (st.span, span_of(n)) {
                (Some(a), Some(b)) => a == b,
                (None, None) => st.score.as_ref() == Some(r),
                _ => false,
            },
        };
        if !same {
            ending.push(n);
        }
    }
    for &n in &ending {
        send_to_sprite(vm, host, n, "endSprite", &[])?;
        host.sprites.remove(&n);
    }
    let mut began = Vec::new();
    for sp in &records {
        let n = sp.number;
        if host.sprites.contains_key(&n) {
            continue; // stays on stage
        }
        let member = (sp.member != 0).then_some((sp.cast_lib as i64, sp.member as i64));
        let mut st = SpriteState {
            member,
            // The score stores the registration point's position.
            // Regression: it was taken as the top-left corner (+ reg point).
            loc: (sp.left as i64, sp.top as i64),
            width: sp.width as i64,
            height: sp.height as i64,
            ink: sp.ink as i64,
            blend: if sp.blend == 0 { 100 } else { sp.blend as i64 },
            visible: true,
            fore_color: sp.fore as i64,
            back_color: sp.back as i64,
            loc_z: n as i64,
            score: Some(sp.clone()),
            stretched: sp.stretch,
            span: span_of(n),
            ..Default::default()
        };
        if let Some(iv) = score.intervals_at(frame).find(|i| i.sprite == n) {
            st.instances = behaviors(vm, host, &iv.behaviors, n);
        }
        host.sprites.insert(n, st);
        began.push(n);
    }
    // The frame script: the script channel's interval.
    let fs_span = span_of(0);
    if fs_span != host.frame_script_span || host.frame_script.is_none() && fs_span.is_some() {
        if let Some(Value::Object(o)) = host.frame_script.take() {
            if vm.has_method(&o, "endSprite") {
                vm.call_method(host, "endSprite", vec![Value::Object(o)])?;
            }
        }
        host.frame_script = score
            .intervals_at(frame)
            .find(|i| i.sprite == 0)
            .and_then(|iv| behaviors(vm, host, &iv.behaviors, 0).into_iter().next());
        host.frame_script_span = fs_span;
        if let Some(Value::Object(o)) = host.frame_script.clone() {
            if vm.has_method(&o, "beginSprite") {
                vm.call_method(host, "beginSprite", vec![Value::Object(o)])?;
            }
        }
    }
    host.frame = frame;
    for &n in &began {
        send_to_sprite(vm, host, n, "beginSprite", &[])?;
    }
    Ok(began)
}

/// The first frame's sprites (startup). See [`enter_frame`].
pub fn begin_frame(
    vm: &mut Vm,
    host: &mut PlayerHost,
    frame: usize,
) -> Result<Vec<u16>, LingoError> {
    enter_frame(vm, host, frame)
}

/// One frame of playback after the startup: the playhead moves to the
/// frame `go` asked for, else the next one (holding on the last), then
/// `stepFrame` (actorList), `prepareFrame`, `enterFrame`, `exitFrame`.
/// `elapsed_ms` advances the movie clock.
pub fn step(vm: &mut Vm, host: &mut PlayerHost, elapsed_ms: i64) -> Result<(), LingoError> {
    host.clock_ms += elapsed_ms.max(0);
    let last = host
        .score
        .as_ref()
        .map(|s| s.frame_count)
        .unwrap_or(1)
        .max(1);
    let next = match host.go_target.take() {
        Some(f) => f,
        None => (host.frame + 1).min(last),
    };
    enter_frame(vm, host, next.clamp(1, last))?;
    // stepFrame to every object in the actorList.
    let actors = match host.settings.get("actorlist") {
        Some(Value::List(l)) => l.borrow().clone(),
        _ => Vec::new(),
    };
    for o in objects(&actors) {
        if vm.has_method(&o, "stepFrame") {
            vm.call_method(host, "stepFrame", vec![Value::Object(o)])?;
        }
    }
    frame_event(vm, host, "prepareFrame")?;
    frame_event(vm, host, "enterFrame")?;
    frame_event(vm, host, "exitFrame")?;
    run_timeouts(vm, host)
}

/// Call the timeouts that are due: `handler(target, timeout)` on an object
/// target, else the movie handler `handler(timeout)`.
// Regression: the TreeView behaviors build their tree from a 100 ms
// timeout (`timeout` was not implemented).
pub fn run_timeouts(vm: &mut Vm, host: &mut PlayerHost) -> Result<(), LingoError> {
    for (id, handler, target) in host.due_timeouts() {
        // Forgotten by an earlier call in this round.
        if host
            .timeouts
            .get(id as usize)
            .is_none_or(|t| t.due.is_none())
        {
            continue;
        }
        let me = Value::Host(lingo_vm::value::HostRef::Object {
            kind: "timeout".into(),
            id,
        });
        match target {
            Value::Object(o) => {
                vm.call_method(host, &handler, vec![Value::Object(o), me])?;
            }
            _ => {
                vm.call(host, &handler, vec![me])?;
            }
        }
    }
    Ok(())
}

/// Send `msg` to the behaviors of sprite `n`. `Some(result)` when at least
/// one behavior handled it (the last result).
pub fn send_to_sprite(
    vm: &mut Vm,
    host: &mut PlayerHost,
    n: u16,
    msg: &str,
    args: &[Value],
) -> Result<Option<Value>, LingoError> {
    let list = host
        .sprites
        .get(&n)
        .map(|s| s.instances.clone())
        .unwrap_or_default();
    let mut out = None;
    for o in objects(&list) {
        if vm.has_method(&o, msg) {
            let mut a = vec![Value::Object(o)];
            a.extend_from_slice(args);
            out = Some(vm.call_method(host, msg, a)?);
        }
    }
    Ok(out)
}

/// `sendSprite(n, #msg, args…)`: the sprite's behaviors; if none handles
/// it, the movie scripts (Director passes it on); unhandled = ignored.
pub fn send_sprite(
    vm: &mut Vm,
    host: &mut PlayerHost,
    n: u16,
    msg: &str,
    args: &[Value],
) -> Result<Value, LingoError> {
    if let Some(v) = send_to_sprite(vm, host, n, msg, args)? {
        return Ok(v);
    }
    movie_fallback(vm, host, msg, args)
}

/// `sendAllSprites(#msg, args…)`.
pub fn send_all_sprites(
    vm: &mut Vm,
    host: &mut PlayerHost,
    msg: &str,
    args: &[Value],
) -> Result<Value, LingoError> {
    let mut handled = false;
    let channels: Vec<u16> = host.sprites.keys().copied().collect();
    for n in channels {
        handled |= send_to_sprite(vm, host, n, msg, args)?.is_some();
    }
    if handled {
        return Ok(Value::Void);
    }
    movie_fallback(vm, host, msg, args)
}

fn movie_fallback(
    vm: &mut Vm,
    host: &mut PlayerHost,
    msg: &str,
    args: &[Value],
) -> Result<Value, LingoError> {
    if vm.program.movie_handler(msg).is_some() {
        return vm.call(host, msg, args.to_vec());
    }
    Ok(Value::Void)
}

/// A mouse event to sprite `n`: its behaviors; if none handles it, the
/// frame script, then the movie scripts (Director's message hierarchy).
pub fn sprite_event(
    vm: &mut Vm,
    host: &mut PlayerHost,
    n: u16,
    msg: &str,
) -> Result<(), LingoError> {
    if n > 0 && send_to_sprite(vm, host, n, msg, &[])?.is_some() {
        return Ok(());
    }
    if let Some(Value::Object(o)) = host.frame_script.clone() {
        if vm.has_method(&o, msg) {
            vm.call_method(host, msg, vec![Value::Object(o)])?;
            return Ok(());
        }
    }
    vm.movie_event(host, msg, vec![])?;
    Ok(())
}

/// What the mouse did (stage coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    Move(i64, i64),
    Down(i64, i64),
    Up(i64, i64),
}

/// A mouse event from the page: moves send `mouseLeave`/`mouseEnter` when
/// the sprite under the pointer changes (`mouseWithin` while it stays);
/// a press sends `mouseDown` to the sprite under it (the clickOn), the
/// release `mouseUp` to that sprite if still over it, else
/// `mouseUpOutside`. Hit testing uses the sprites' rectangles (⏳ matte
/// ink uses the bitmap's mask in Director).
pub fn mouse(vm: &mut Vm, host: &mut PlayerHost, ev: Mouse) -> Result<(), LingoError> {
    let (x, y) = match ev {
        Mouse::Move(x, y) | Mouse::Down(x, y) | Mouse::Up(x, y) => (x, y),
    };
    host.mouse = (x, y);
    let under = host.sprite_under((x, y), None).unwrap_or(0) as u16;
    match ev {
        Mouse::Move(..) => {
            let before = host.mouse_over;
            if before != under {
                host.mouse_over = under;
                if before > 0 && host.sprites.contains_key(&before) {
                    send_to_sprite(vm, host, before, "mouseLeave", &[])?;
                }
                if under > 0 {
                    send_to_sprite(vm, host, under, "mouseEnter", &[])?;
                }
            } else if under > 0 {
                send_to_sprite(vm, host, under, "mouseWithin", &[])?;
            }
        }
        Mouse::Down(..) => {
            host.mouse_down = true;
            host.click_on = under;
            sprite_event(vm, host, under, "mouseDown")?;
        }
        Mouse::Up(..) => {
            host.mouse_down = false;
            let down = host.click_on;
            if down == under {
                sprite_event(vm, host, under, "mouseUp")?;
            } else if down > 0 && host.sprites.contains_key(&down) {
                send_to_sprite(vm, host, down, "mouseUpOutside", &[])?;
            }
        }
    }
    Ok(())
}

/// A key from the page. `key` is Director's character (`\u{8}` backspace,
/// `\r` return, `\t` tab, 28–31 arrows); `code` the Mac key code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub down: bool,
    pub key: String,
    pub code: i64,
    pub shift: bool,
    pub option: bool,
    pub command: bool,
    pub control: bool,
}

/// A key event: `keyDown`/`keyUp` to the sprite with the keyboard focus
/// (its behaviors), else the frame script, else the movie scripts. An
/// editable field with the focus then gets the typed key unless a handler
/// took it without `pass` (or called `dontPassEvent`/`stopEvent`).
// Regression: the page sent no keys (search fields could not be typed in).
pub fn key(vm: &mut Vm, host: &mut PlayerHost, k: &Key) -> Result<(), LingoError> {
    host.key = k.clone();
    host.pass_event = None;
    let msg = if k.down { "keyDown" } else { "keyUp" };
    let focus = host
        .settings
        .get("keyboardfocussprite")
        .map(|v| v.to_int())
        .filter(|&n| n > 0 && host.sprites.contains_key(&(n as u16)))
        .unwrap_or(0) as u16;
    let mut handled = focus > 0 && send_to_sprite(vm, host, focus, msg, &[])?.is_some();
    if !handled {
        if let Some(Value::Object(o)) = host.frame_script.clone() {
            if vm.has_method(&o, msg) {
                vm.call_method(host, msg, vec![Value::Object(o)])?;
                handled = true;
            }
        }
    }
    if !handled && vm.program.movie_handler(msg).is_some() {
        vm.call(host, msg, vec![])?;
        handled = true;
    }
    let typed = match host.pass_event {
        Some(pass) => pass,
        None => !handled,
    };
    if k.down && typed && focus > 0 {
        host.type_into_sprite(focus, &k.key);
    }
    Ok(())
}

/// A frame event (`prepareFrame`, `enterFrame`, `exitFrame`): every
/// sprite's behaviors, then the frame script, else the movie scripts.
pub fn frame_event(vm: &mut Vm, host: &mut PlayerHost, msg: &str) -> Result<(), LingoError> {
    let channels: Vec<u16> = host.sprites.keys().copied().collect();
    for n in channels {
        send_to_sprite(vm, host, n, msg, &[])?;
    }
    if let Some(Value::Object(o)) = host.frame_script.clone() {
        if vm.has_method(&o, msg) {
            vm.call_method(host, msg, vec![Value::Object(o)])?;
            return Ok(());
        }
    }
    vm.movie_event(host, msg, vec![])?;
    Ok(())
}

/// The movie's startup, step by step: (step name, result). Stops at the
/// first error, which is the last entry.
pub fn start_movie(
    vm: &mut Vm,
    host: &mut PlayerHost,
) -> Vec<(&'static str, Result<(), LingoError>)> {
    let mut out: Vec<(&'static str, Result<(), LingoError>)> = Vec::new();
    macro_rules! step {
        ($name:expr, $e:expr) => {{
            let r: Result<(), LingoError> = $e;
            let ok = r.is_ok();
            out.push(($name, r));
            if !ok {
                return out;
            }
        }};
    }
    step!(
        "prepareMovie",
        vm.movie_event(host, "prepareMovie", vec![]).map(|_| ())
    );
    step!("beginSprite", begin_frame(vm, host, 1).map(|_| ()));
    step!("prepareFrame", frame_event(vm, host, "prepareFrame"));
    step!(
        "startMovie",
        vm.movie_event(host, "startMovie", vec![]).map(|_| ())
    );
    step!("enterFrame", frame_event(vm, host, "enterFrame"));
    step!("exitFrame", frame_event(vm, host, "exitFrame"));
    out
}
