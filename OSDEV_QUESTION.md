# LLVM optimizing away loops with atomic stores in release mode - expected behavior?

Hey folks, working on a Rust ARM64 microkernel and hitting a weird optimization issue. Looking for sanity check on whether this is expected LLVM behavior or if I'm doing something wrong.

## Setup
- Rust nightly (LLVM 21.1.2), `aarch64-unknown-none` target
- `no_std`, `--release` build
- Console buffer shared between processes via IPC (non-cacheable memory region)

## Code
```rust
struct Console {
    buffer: [[AtomicU8; 64]; 38],  // 64x38 character grid
}

fn clear(&mut self) {
    for y in 0..38 {
        for x in 0..64 {
            self.buffer[y][x].store(b' ', Ordering::Release);
        }
    }
}
```

## Problem
In release mode, this loop appears to execute (I can add a counter that reaches 2432), but the actual memory is unchanged - old text remains visible on screen.

## What works
```rust
fn clear(&mut self) {
    let h = core::hint::black_box(38);
    let w = core::hint::black_box(64);
    for y in 0..h {
        for x in 0..w {
            self.buffer[y][x].store(b' ', Ordering::Release);
        }
    }
}
```

## Questions
1. Is it expected that LLVM can optimize away loop iterations even with `AtomicU8::store(Ordering::Release)`? I thought atomics prevented this kind of elimination.
2. The memory region is marked non-cacheable in page tables - could LLVM be doing something weird because it doesn't know about the memory attributes?
3. Is `black_box` on loop bounds the idiomatic fix, or is there a better way to express "this loop must fully execute"?

Debug mode works fine, it's specifically `-O` optimization causing this. Any insights appreciated!
