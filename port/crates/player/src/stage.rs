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

/// Enter frame `frame` (1-based) of the loaded score: set up its sprites
/// and behaviors (when the playhead arrives from elsewhere) and send
/// `beginSprite`. Returns the sprites that began.
pub fn begin_frame(
    vm: &mut Vm,
    host: &mut PlayerHost,
    frame: usize,
) -> Result<Vec<u16>, LingoError> {
    let Some(score) = host.score.clone() else {
        return Ok(Vec::new());
    };
    let mut began = Vec::new();
    let sprites = score.sprites(frame).unwrap_or_default();
    for sp in &sprites {
        let n = sp.number;
        if host
            .sprites
            .get(&n)
            .is_some_and(|s| !s.instances.is_empty() || s.puppet)
        {
            continue; // already on stage
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
            ..Default::default()
        };
        for iv in score.intervals_at(frame).filter(|i| i.sprite == n) {
            for b in &iv.behaviors {
                let Some(script) = host
                    .member_info(b.cast_lib as i64, b.member as i64)
                    .and_then(|m| m.script)
                else {
                    continue;
                };
                let p = params(b.params.as_deref());
                st.instances.push(vm.new_behavior(script, &p, n as i64));
            }
        }
        host.sprites.insert(n, st);
        began.push(n);
    }
    // The frame script is a behavior of the script channel.
    host.frame_script = score
        .frame_script(frame)
        .and_then(|(l, m)| host.member_info(l as i64, m as i64))
        .and_then(|m| m.script)
        .map(|s| vm.new_behavior(s, &[], 0));
    for &n in &began {
        send_to_sprite(vm, host, n, "beginSprite", &[])?;
    }
    if let Some(Value::Object(o)) = host.frame_script.clone() {
        if vm.has_method(&o, "beginSprite") {
            vm.call_method(host, "beginSprite", vec![Value::Object(o)])?;
        }
    }
    host.frame = frame;
    Ok(began)
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
