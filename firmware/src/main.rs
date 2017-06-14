#![deny(warnings)]
#![feature(const_fn)]
#![feature(used)]
#![no_std]

extern crate blue_pill;
extern crate cast;
extern crate cortex_m_rt;
#[macro_use]
extern crate cortex_m_rtfm as rtfm;
extern crate embedded_hal as hal;
extern crate nb;
extern crate shared;

use core::cell::Cell;

use blue_pill::capture::Event;
use blue_pill::dma::{Buffer, Dma1Channel4, Dma1Channel5};
use blue_pill::gpio::{self, PB12, PB13, PB14, PB15};
use blue_pill::led::{self, Green};
use blue_pill::stm32f103xx;
use blue_pill::time::{Hertz, Microseconds};
use blue_pill::timer::Channel;
use blue_pill::{Capture, Pwm, Serial, Timer};
use cast::i32;
use hal::prelude::*;
use rtfm::{C1, Local, P0, P1, Resource, T0, T1, TMax};
use shared::{Command, State};
use stm32f103xx::interrupt::{DMA1_CHANNEL4, DMA1_CHANNEL5, TIM1_UP_TIM10, TIM4};

// CONFIGURATION
const BAUD_RATE: Hertz = Hertz(115_200);
const CAPTURE_RESOLUTION: Microseconds = Microseconds(100);
const PWM_FREQUENCY: Hertz = Hertz(1_000);
const TIMER_FREQUENCY: Hertz = Hertz(1);
// Rust too op; please nerf
const NERF_NUM: i32 = 3;
const NERF_DEN: i32 = 4;

// RESOURCES
peripherals!(stm32f103xx, {
    AFIO: Peripheral { ceiling: C0, },
    DMA1: Peripheral { ceiling: C1, },
    DWT: Peripheral { ceiling: C0, },
    GPIOA: Peripheral { ceiling: C0, },
    GPIOB: Peripheral { ceiling: C0, },
    GPIOC: Peripheral { ceiling: C0, },
    RCC: Peripheral { ceiling: C0, },
    TIM1: Peripheral { ceiling: C1, },
    TIM3: Peripheral { ceiling: C1, },
    TIM4: Peripheral { ceiling: C1, },
    USART1: Peripheral { ceiling: C1, },
});

static ACTIVE: Resource<Cell<bool>, C1> = Resource::new(Cell::new(false));
static DUTY_L: Resource<Cell<i16>, C1> = Resource::new(Cell::new(0));
static DUTY_R: Resource<Cell<i16>, C1> = Resource::new(Cell::new(0));
static POS_L: Resource<Cell<u8>, C1> = Resource::new(Cell::new(0));
static POS_R: Resource<Cell<u8>, C1> = Resource::new(Cell::new(0));
static RX_BUFFER: Resource<Buffer<[u8; 3], Dma1Channel5>, C1> =
    Resource::new(Buffer::new([0; 3]));
static TX_BUFFER: Resource<Buffer<[u8; 11], Dma1Channel4>, C1> =
    Resource::new(Buffer::new([0; 11]));
static SLEEP_CYCLES: Resource<Cell<u32>, C1> = Resource::new(Cell::new(0));

// INITIALIZATION
fn init(ref prio: P0, thr: &TMax) {
    let afio = &AFIO.access(prio, thr);
    let dma1 = &DMA1.access(prio, thr);
    let dwt = &DWT.access(prio, thr);
    let gpioa = &GPIOA.access(prio, thr);
    let gpiob = &GPIOB.access(prio, thr);
    let gpioc = &GPIOC.access(prio, thr);
    let rcc = &RCC.access(prio, thr);
    let rx_buffer = RX_BUFFER.access(prio, thr);
    let tim1 = TIM1.access(prio, thr);
    let tim3 = TIM3.access(prio, thr);
    let tim4 = TIM4.access(prio, thr);
    let usart1 = USART1.access(prio, thr);

    let capture = Capture(&*tim4);
    let pwm = Pwm(&*tim3);
    let serial = Serial(&*usart1);
    let timer = Timer(&*tim1);

    capture.init(CAPTURE_RESOLUTION, afio, gpiob, rcc);
    dwt.enable_cycle_counter();
    gpio::init(gpiob, rcc);
    led::init(gpioc, rcc);
    pwm.init(PWM_FREQUENCY.invert(), afio, gpioa, rcc);
    serial.init(BAUD_RATE.invert(), afio, Some(dma1), gpioa, rcc);
    timer.init(TIMER_FREQUENCY.invert(), rcc);

    capture.enable(Channel::_1);
    capture.enable(Channel::_2);
    capture.listen(Event::Capture1);
    capture.listen(Event::Capture2);

    serial.read_exact(dma1, rx_buffer).unwrap();
    timer.resume();
}

// IDLE LOOP: CPU MONITOR
fn idle(ref prio: P0, _thr: T0) -> ! {
    loop {
        rtfm::atomic(|thr| {
            let dwt = DWT.access(prio, thr);
            let sleep_cycles = SLEEP_CYCLES.access(prio, thr);

            let before = dwt.cyccnt.read();
            rtfm::wfi();
            let after = dwt.cyccnt.read();

            let elapsed = after.wrapping_sub(before);
            sleep_cycles.set(sleep_cycles.get() + elapsed);
        });
    }
}

// TASKS
tasks!(stm32f103xx, {
    capture: Task {
        interrupt: TIM4,
        priority: P1,
        enabled: true,
    },
    log: Task {
        interrupt: TIM1_UP_TIM10,
        priority: P1,
        enabled: true,
    },
    rx: Task {
        interrupt: DMA1_CHANNEL5,
        priority: P1,
        enabled: true,
    },
    tx: Task {
        interrupt: DMA1_CHANNEL4,
        priority: P1,
        enabled: true,
    },
});

/// Measure motor speed
fn capture(_task: TIM4, ref prio: P1, ref thr: T1) {
    let pos_l = POS_L.access(prio, thr);
    let pos_r = POS_R.access(prio, thr);
    let tim4 = TIM4.access(prio, thr);

    let capture = Capture(&*tim4);

    match capture.capture(Channel::_1) {
        Err(nb::Error::Other(_)) => panic!(), // overcapture
        Err(nb::Error::WouldBlock) => {}
        Ok(_) => {
            pos_l.set(pos_l.get() + 1);
        }
    }

    match capture.capture(Channel::_2) {
        Err(nb::Error::Other(_)) => panic!(), // overcapture
        Err(nb::Error::WouldBlock) => {}
        Ok(_) => {
            pos_r.set(pos_r.get() + 1);
        }
    }
}

/// Log state periodically
fn log(_task: TIM1_UP_TIM10, ref prio: P1, ref thr: T1) {
    let active = ACTIVE.access(prio, thr);
    let buffer = TX_BUFFER.access(prio, thr);
    let dma1 = &DMA1.access(prio, thr);
    let duty_l = DUTY_L.access(prio, thr);
    let duty_r = DUTY_R.access(prio, thr);
    let pos_l = POS_L.access(prio, thr);
    let pos_r = POS_R.access(prio, thr);
    let sleep_cycles = SLEEP_CYCLES.access(prio, thr);
    let tim1 = TIM1.access(prio, thr);
    let usart1 = USART1.access(prio, thr);

    let timer = Timer(&*tim1);
    let serial = Serial(&*usart1);

    // clear the update flag
    timer.wait().unwrap();

    if active.get() {
        let state = State {
            duty_left: duty_l.get(),
            duty_right: duty_r.get(),
            sleep_cycles: sleep_cycles.get(),
            speed_left: pos_l.get(),
            speed_right: pos_r.get(),
        };
        state.serialize(&mut *buffer.borrow_mut());

        serial.write_all(dma1, buffer).unwrap();
    }

    pos_l.set(0);
    pos_r.set(0);
    sleep_cycles.set(0);
}

/// New command arrived
fn rx(ref task: DMA1_CHANNEL5, ref prio: P1, ref thr: T1) {
    static X: Local<Cell<i16>, DMA1_CHANNEL5> = Local::new(Cell::new(0));
    static Y: Local<Cell<i16>, DMA1_CHANNEL5> = Local::new(Cell::new(0));

    let active = ACTIVE.access(prio, thr);
    let buffer = RX_BUFFER.access(prio, thr);
    let dma1 = &DMA1.access(prio, thr);
    let duty_l = DUTY_L.access(prio, thr);
    let duty_r = DUTY_R.access(prio, thr);
    let tim3 = TIM3.access(prio, thr);
    let usart1 = USART1.access(prio, thr);
    let x = X.borrow(task);
    let y = Y.borrow(task);

    let serial = Serial(&*usart1);
    let pwm = Pwm(&*tim3);

    buffer.release(dma1).unwrap();

    let cmd = {
        let buffer = buffer.borrow();
        Command::deserialize(&buffer)
    };

    // queue a new read
    serial.read_exact(dma1, buffer).unwrap();

    match cmd {
        Ok(Command::Start) => {
            if active.get() {
                Green.off();

                active.set(false);
                x.set(0);
                y.set(0);

                // Hand brake
                LeftMotor.brake();
                RightMotor.brake();

                pwm.disable(Channel::_1);
                pwm.disable(Channel::_2);
            } else {
                Green.on();

                pwm.enable(Channel::_1);
                pwm.enable(Channel::_2);

                active.set(true);
            }

            return;
        }
        Ok(Command::X(v)) if active.get() => x.set(v),
        Ok(Command::Y(v)) if active.get() => y.set(v),
        _ => return,
    }

    if active.get() {
        // NOTE y < 0 means forwards, x > 0 means turn right
        let (x, y) = (i32(x.get()), i32(y.get()));

        // NOTE speed > 0 means forwards
        let speed = ((i32(pwm.get_max_duty()) * -y / NERF_DEN) * NERF_NUM) >>
            15;

        let (speed_l, speed_r) = if x > 0 {
            (speed, (speed * ((1 << 15) - 2 * x)) >> 15)
        } else {
            ((speed * ((1 << 15) + 2 * x)) >> 15, speed)
        };

        if speed_l > 0 {
            LeftMotor.forward()
        } else {
            LeftMotor.backward()
        }
        duty_l.set(speed_l as i16);
        LeftMotor.duty(&tim3, speed_l.abs() as u16);

        if speed_r > 0 {
            RightMotor.forward()
        } else {
            RightMotor.backward()
        }
        duty_r.set(speed_r as i16);
        RightMotor.duty(&tim3, speed_r.abs() as u16);
    }
}

/// DMA transfer finished
fn tx(_task: DMA1_CHANNEL4, ref prio: P1, ref thr: T1) {
    let buffer = TX_BUFFER.access(prio, thr);
    let dma1 = &DMA1.access(prio, thr);

    buffer.release(dma1).unwrap();
}

// AUXILIARY
struct LeftMotor;

impl LeftMotor {
    fn backward(&self) {
        PB12.low();
        PB13.high();
    }

    fn brake(&self) {
        PB12.low();
        PB13.low();
    }

    fn duty(&self, tim3: &stm32f103xx::TIM3, duty: u16) {
        Pwm(tim3).set_duty(Channel::_2, duty);
    }

    fn forward(&self) {
        PB12.high();
        PB13.low();
    }
}

struct RightMotor;

impl RightMotor {
    fn backward(&self) {
        PB14.low();
        PB15.high();
    }

    fn brake(&self) {
        PB14.low();
        PB15.low();
    }

    fn duty(&self, tim3: &stm32f103xx::TIM3, duty: u16) {
        Pwm(tim3).set_duty(Channel::_1, duty);
    }

    fn forward(&self) {
        PB14.high();
        PB15.low();
    }
}
