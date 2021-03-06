#![deny(warnings)]
#![feature(const_fn)]
#![feature(proc_macro)]
#![feature(used)]
#![no_std]

extern crate blue_pill;
extern crate cast;
extern crate cortex_m_rt;
extern crate cortex_m_rtfm as rtfm;
extern crate nb;
extern crate shared;

use blue_pill::capture::Event;
use blue_pill::dma::{Buffer, Dma1Channel4, Dma1Channel5};
use blue_pill::gpio::{self, PB12, PB13, PB14, PB15};
use blue_pill::led::{self, Green};
use blue_pill::prelude::*;
use blue_pill::stm32f103xx;
use blue_pill::time::{Hertz, Microseconds};
use blue_pill::{Capture, Channel, Pwm, Serial, Timer};
use cast::i32;
use rtfm::{app, Resource, Threshold};
use shared::{Command, State};

// CONFIGURATION
const BAUD_RATE: Hertz = Hertz(115_200);
const CAPTURE_RESOLUTION: Microseconds = Microseconds(100);
const PWM_FREQUENCY: Hertz = Hertz(1_000);
const TIMER_FREQUENCY: Hertz = Hertz(1);
// If the input voltage is too high use this to limit to voltage applied to the
// motor
const DOWNVOLTAGE_NUM: i32 = 3;
const DOWNVOLTAGE_DEN: i32 = 4;

// TASKS AND RESOURCES
app! {
    device: blue_pill::stm32f103xx,

    resources: {
        static ACTIVE: bool = false;
        static DUTY_L: i16 = 0;
        static DUTY_R: i16 = 0;
        static POS_L: u8 = 0;
        static POS_R: u8 = 0;
        static RX_BUFFER: Buffer<[u8; 3], Dma1Channel5> = Buffer::new([0; 3]);
        static SLEEP_CYCLES: u32 = 0;
        static TX_BUFFER: Buffer<[u8; 11], Dma1Channel4> = Buffer::new([0; 11]);
        static X: i16 = 0;
        static Y: i16 = 0;
    },

    idle: {
        resources: [DWT, SLEEP_CYCLES],
    },

    tasks: {
        DMA1_CHANNEL4: {
            path: tx_transfer_done,
            resources: [DMA1, TX_BUFFER],
        },

        DMA1_CHANNEL5: {
            path: rx,
            resources: [
                ACTIVE,
                DMA1,
                DUTY_L,
                DUTY_R,
                RX_BUFFER,
                TIM3,
                USART1,
                X,
                Y,
            ],
        },

        TIM1_UP_TIM10: {
            path: log,
            resources: [
                ACTIVE,
                DMA1,
                DUTY_L,
                DUTY_R,
                POS_L,
                POS_R,
                SLEEP_CYCLES,
                TIM1,
                TX_BUFFER,
                USART1,
            ],
        },

        TIM4: {
            path: capture,
            resources: [POS_L, POS_R, TIM4],
        },
    },
}

// INITIALIZATION
fn init(p: init::Peripherals, r: init::Resources) {
    let capture = Capture(p.TIM4);
    let pwm = Pwm(p.TIM3);
    let serial = Serial(p.USART1);
    let timer = Timer(p.TIM1);

    capture.init(CAPTURE_RESOLUTION, p.AFIO, p.GPIOB, p.RCC);
    p.DWT.enable_cycle_counter();
    gpio::init(p.GPIOB, p.RCC);
    led::init(p.GPIOC, p.RCC);
    pwm.init(PWM_FREQUENCY.invert(), p.AFIO, None, p.GPIOA, p.RCC);
    serial.init(BAUD_RATE.invert(), p.AFIO, Some(p.DMA1), p.GPIOA, p.RCC);
    timer.init(TIMER_FREQUENCY.invert(), p.RCC);

    capture.enable(Channel::_1);
    capture.enable(Channel::_2);
    capture.listen(Event::Capture1);
    capture.listen(Event::Capture2);

    serial.read_exact(p.DMA1, r.RX_BUFFER).unwrap();
    timer.resume();
}

// IDLE LOOP: CPU MONITOR
fn idle(t: &mut Threshold, mut r: idle::Resources) -> ! {
    loop {
        rtfm::atomic(t, |t| {
            let sleep_cycles = r.SLEEP_CYCLES.borrow_mut(t);

            let before = r.DWT.cyccnt.read();
            rtfm::wfi();
            let after = r.DWT.cyccnt.read();

            let elapsed = after.wrapping_sub(before);
            **sleep_cycles += elapsed;
        });
    }
}

// TASKS

// Measure motor speed
fn capture(_t: &mut Threshold, r: TIM4::Resources) {
    let capture = Capture(&**r.TIM4);

    match capture.capture(Channel::_1) {
        Err(nb::Error::Other(_)) => panic!(), // overcapture
        Err(nb::Error::WouldBlock) => {}
        Ok(_) => {
            **r.POS_L += 1;
        }
    }

    match capture.capture(Channel::_2) {
        Err(nb::Error::Other(_)) => panic!(), // overcapture
        Err(nb::Error::WouldBlock) => {}
        Ok(_) => {
            **r.POS_R += 1;
        }
    }
}

// New command arrived
fn rx(_t: &mut Threshold, r: DMA1_CHANNEL5::Resources) {
    let serial = Serial(&**r.USART1);
    let pwm = Pwm(&**r.TIM3);

    r.RX_BUFFER.release(r.DMA1).unwrap();

    let cmd = Command::deserialize(&(**r.RX_BUFFER).borrow());

    // queue a new read
    serial.read_exact(r.DMA1, r.RX_BUFFER).unwrap();

    match cmd {
        Ok(Command::Start) => {
            if **r.ACTIVE {
                Green.off();

                **r.ACTIVE = false;
                **r.X = 0;
                **r.Y = 0;

                // Hand brake
                LeftMotor.brake();
                RightMotor.brake();

                pwm.disable(Channel::_1);
                pwm.disable(Channel::_2);
            } else {
                Green.on();

                pwm.enable(Channel::_1);
                pwm.enable(Channel::_2);

                **r.ACTIVE = true;
            }

            return;
        }
        Ok(Command::X(v)) if **r.ACTIVE => **r.X = v,
        Ok(Command::Y(v)) if **r.ACTIVE => **r.Y = v,
        _ => return,
    }

    if **r.ACTIVE {
        // NOTE y < 0 means forwards, x > 0 means turn right
        let (x, y) = (i32(**r.X), i32(**r.Y));

        // NOTE speed > 0 means forwards
        let speed = ((i32(pwm.get_max_duty()) * -y / DOWNVOLTAGE_DEN) *
            DOWNVOLTAGE_NUM) >> 15;

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
        **r.DUTY_L = speed_l as i16;
        LeftMotor.duty(r.TIM3, speed_l.abs() as u16);

        if speed_r > 0 {
            RightMotor.forward()
        } else {
            RightMotor.backward()
        }
        **r.DUTY_R = speed_r as i16;
        RightMotor.duty(r.TIM3, speed_r.abs() as u16);
    }
}

// Log state periodically
fn log(_t: &mut Threshold, r: TIM1_UP_TIM10::Resources) {
    let timer = Timer(&**r.TIM1);
    let serial = Serial(&**r.USART1);

    // clear the update flag
    timer.wait().unwrap();

    if **r.ACTIVE {
        let state = State {
            duty_left: **r.DUTY_L,
            duty_right: **r.DUTY_R,
            sleep_cycles: **r.SLEEP_CYCLES,
            speed_left: **r.POS_L,
            speed_right: **r.POS_R,
        };
        state.serialize(&mut (**r.TX_BUFFER).borrow_mut());

        serial.write_all(r.DMA1, r.TX_BUFFER).unwrap();
    }

    **r.POS_L = 0;
    **r.POS_R = 0;
    **r.SLEEP_CYCLES = 0;
}

fn tx_transfer_done(_t: &mut Threshold, r: DMA1_CHANNEL4::Resources) {
    r.TX_BUFFER.release(r.DMA1).unwrap();
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
