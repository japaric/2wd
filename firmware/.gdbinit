target remote :3333

# monitor arm semihosting enable

monitor tpiu config external uart off 8000000 2000000
monitor itm port 0 on

load
step
