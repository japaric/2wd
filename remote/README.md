# `remote`

> Tool to remotely control the robot

## Usage

``` console
$ cat /dev/input/js0 | remote > /dev/rfcomm0
```

- `/dev/input/js0` is a gamepad. Tested with a sixaxis controller.
- `/dev/rfcomm0` is an RFCOMM port paired with the robot.

## Controls

- Start button: Turns the robot on / off. When the robot is off it ignores other
  commands and doesn't log data.

- Left stick's Y axis: Moves the robot forward / backward.

- Right stick's X axis: Turns the robot left / right. Note that this control
  doesn't move the robot by itself; it must be paired with some forward /
  backward motion.

Other controls are ignored.
