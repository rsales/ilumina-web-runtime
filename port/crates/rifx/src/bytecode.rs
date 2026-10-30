//! Lingo bytecode disassembly (no decompilation).
//!
//! Encoding: one opcode byte. Opcodes below 0x40 take no argument. From
//! 0x40 up, the top two bits give the argument width (0x40: 1 byte,
//! 0x80: 2 bytes, 0xC0: 4 bytes) and `0x40 | (op & 0x3F)` is the base opcode.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Op(pub u8);

#[derive(Clone, Copy, Debug)]
pub struct Instr {
    pub offset: usize,
    pub op: Op,
    pub arg: i32,
}

impl Op {
    pub fn name(self) -> &'static str {
        match self.0 {
            0x01 => "ret",
            0x02 => "retfactory",
            0x03 => "pushzero",
            0x04 => "mul",
            0x05 => "add",
            0x06 => "sub",
            0x07 => "div",
            0x08 => "mod",
            0x09 => "inv",
            0x0a => "joinstr",
            0x0b => "joinpadstr",
            0x0c => "lt",
            0x0d => "lteq",
            0x0e => "nteq",
            0x0f => "eq",
            0x10 => "gt",
            0x11 => "gteq",
            0x12 => "and",
            0x13 => "or",
            0x14 => "not",
            0x15 => "containsstr",
            0x16 => "contains0str",
            0x17 => "getchunk",
            0x18 => "hilitechunk",
            0x19 => "ontospr",
            0x1a => "intospr",
            0x1b => "getfield",
            0x1c => "starttell",
            0x1d => "endtell",
            0x1e => "pushlist",
            0x1f => "pushproplist",
            0x21 => "swap",
            0x41 => "pushint8",
            0x42 => "pusharglistnoret",
            0x43 => "pusharglist",
            0x44 => "pushcons",
            0x45 => "pushsymb",
            0x46 => "pushvarref",
            0x48 => "getglobal2",
            0x49 => "getglobal",
            0x4a => "getprop",
            0x4b => "getparam",
            0x4c => "getlocal",
            0x4e => "setglobal2",
            0x4f => "setglobal",
            0x50 => "setprop",
            0x51 => "setparam",
            0x52 => "setlocal",
            0x53 => "jmp",
            0x54 => "endrepeat",
            0x55 => "jmpifz",
            0x56 => "localcall",
            0x57 => "extcall",
            0x58 => "objcallv4",
            0x59 => "put",
            0x5a => "putchunk",
            0x5b => "deletechunk",
            0x5c => "get",
            0x5d => "set",
            0x5f => "getmovieprop",
            0x60 => "setmovieprop",
            0x61 => "getobjprop",
            0x62 => "setobjprop",
            0x63 => "tellcall",
            0x64 => "peek",
            0x65 => "pop",
            0x66 => "thebuiltin",
            0x67 => "objcall",
            0x6d => "pushchunkvarref",
            0x6e => "pushint16",
            0x6f => "pushint32",
            0x70 => "getchainedprop",
            0x71 => "pushfloat32",
            0x72 => "gettoplevelprop",
            0x73 => "newobj",
            _ => "unknown",
        }
    }

    /// True when the argument is an index into the context's name table.
    pub fn arg_is_name(self) -> bool {
        matches!(
            self.0,
            0x45 | 0x46
                | 0x48
                | 0x49
                | 0x4a
                | 0x4e
                | 0x4f
                | 0x50
                | 0x57
                | 0x58
                | 0x5f
                | 0x60
                | 0x61
                | 0x62
                | 0x63
                | 0x66
                | 0x67
                | 0x70
                | 0x72
                | 0x73
        )
    }
}

pub fn disassemble(code: &[u8]) -> Result<Vec<Instr>, Error> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let offset = i;
        let raw = code[i];
        i += 1;
        let (op, width) = match raw {
            0x00..=0x3f => (raw, 0),
            0x40..=0x7f => (raw, 1),
            0x80..=0xbf => (0x40 | (raw & 0x3f), 2),
            _ => (0x40 | (raw & 0x3f), 4),
        };
        let bytes = code
            .get(i..i + width)
            .ok_or(Error::Eof { at: i, need: width })?;
        let arg = match width {
            0 => 0,
            1 => bytes[0] as i8 as i32 & 0xff,
            2 => i16::from_be_bytes([bytes[0], bytes[1]]) as i32,
            _ => i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        };
        // pushint8 is signed; everything else with a 1-byte arg is unsigned.
        let arg = if op == 0x41 && width == 1 {
            bytes[0] as i8 as i32
        } else {
            arg
        };
        i += width;
        out.push(Instr {
            offset,
            op: Op(op),
            arg,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_base_opcodes() {
        // pushint8 -1, pushcons (2-byte) 300, extcall (4-byte) 7, ret
        let code = [0x41, 0xff, 0x84, 0x01, 0x2c, 0xd7, 0, 0, 0, 7, 0x01];
        let ins = disassemble(&code).unwrap();
        let v: Vec<_> = ins.iter().map(|i| (i.op.name(), i.arg)).collect();
        assert_eq!(
            v,
            [
                ("pushint8", -1),
                ("pushcons", 300),
                ("extcall", 7),
                ("ret", 0)
            ]
        );
    }

    #[test]
    fn truncated_argument_is_an_error() {
        assert!(disassemble(&[0x84, 0x01]).is_err());
    }
}
