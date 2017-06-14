extern crate shared;

use std::io::{self, Read, Write};

use shared::Command;

const BUTTON_PRESSED: i16 = 1;

// See https://www.kernel.org/doc/Documentation/input/joystick-api.txt
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Event {
    pub time: u32,
    pub value: i16,
    pub ty: u8,
    pub number: u8,
}

#[derive(Debug, PartialEq)]
enum Type {
    Button(u8),
    Other,
    XAxis(u8),
    YAxis(u8),
}

impl Event {
    fn from(buffer: &[u8; 8]) -> Self {
        unsafe { *(buffer as *const _ as *const Event) }
    }

    fn ty(&self) -> Type {
        match self.ty {
            0x01 => Type::Button(self.number),
            0x02 => {
                if self.number & 1 == 0 {
                    Type::XAxis(self.number / 2)
                } else {
                    Type::YAxis(self.number / 2)
                }
            }
            _ => Type::Other,
        }
    }
}

fn main() {
    run().unwrap();
}

fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();

    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    let mut input = [0; 8];
    let mut output = [0; 3];

    loop {
        stdin.read_exact(&mut input)?;

        let ev = Event::from(&input);
        let ty = ev.ty();

        writeln!(stderr, "{:?} - {}", ev.ty(), ev.value)?;

        let cmd = match ty {
            Type::Button(3) if ev.value == BUTTON_PRESSED => Command::Start,
            Type::XAxis(1) => Command::X(ev.value),
            Type::YAxis(0) => Command::Y(ev.value),
            _ => continue,
        };

        cmd.serialize(&mut output);
        stdout.write_all(&output)?;
        stdout.flush()?;
    }
}
