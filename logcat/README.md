# `logcat`

> Decodes the robot log frames

## Usage

``` console
$ cat /dev/rfcomm0 | logcat
CPU: 0.22% - SL: 105 - SR: 104 - DL: -6000 - DR: -6000
CPU: 0.21% - SL: 99 - SR: 102 - DL: 5999 - DR: 5999
```

- `/dev/rfcomm0` is an RFCOMM port paired with the robot.
- `CPU: 0.21%` is the current CPU use
- `SL: 105 - SR: 104` are the left and right motor speeds.
- `DL: 5999 - DR: 5999` are the PWM duty cycles of the left and right motors.
