# `firmware`

> Robot's firmware

# Wiring

All voltages are 3V3 unless otherwise noted

## Actuators

- DC motors: L298N x2
    - Right motor
        - IN3: PB14 (GPIO)
        - IN4: PB15 (GPIO)
        - ENB: PA6 (T3C1 - PWM)
    - Left motor
        - IN1: PB12 (GPIO)
        - IN2: PB13 (GPIO)
        - ENA: PA7 (T3C2 - PWM)

## Sensors

- Speed: optical incremental encoder x2
    - Left motor
        - DO: PB6 (T4C1 - Input Capture)
    - Right motor
        - DO: PB7 (T4C2 - Input Capture)

- Distance: Ultrasonic sensor
    - VCC = 5V
    - Trigger: P?? (GPIO)
    - Echo: PB8 (T4C3 - Input Capture) (5V tolerant pin)

## Communication

- Bluetooth (RFCOMM)
    - VCC = 5V
    - TX: PA9 (USART1)
    - RX: PA10 (USART1)
