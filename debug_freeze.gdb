# GDB script to debug the freeze
target remote localhost:1234

# Show current state
echo \n=== CURRENT STATE ===\n
info registers

# Show current instruction
echo \n=== CURRENT INSTRUCTION ===\n
x/10i $pc

# Show exception level
echo \n=== EXCEPTION LEVEL ===\n
p/x $CurrentEL

# Show stack
echo \n=== STACK ===\n
x/40xg $sp

# Try to get backtrace
echo \n=== BACKTRACE ===\n
bt

echo \n=== READY FOR COMMANDS ===\n
echo Try: continue, stepi, info threads, x/100i $pc\n
