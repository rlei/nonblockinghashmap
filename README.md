Highly Scalable Non-blocking Hash Map in Rust
--------
[![Build Status](https://travis-ci.com/rlei/nonblockinghashmap.svg?branch=master)](https://travis-ci.com/rlei/nonblockinghashmap)

*WARNING*: this library is still far from ready for any use, but PRs are always welcome of course!

The design of this hash map is taken and modified from [Dr. Cliff Click's design], [originally implemented in Java]. The hash map is thread-safe, thus can be safely used as a shared resource among multiple threads, without requiring mutual exclusion, and typesafe, because it is implemented in Rust. It is also scalable and can be shared among a large number of threads without experincing a major bottleneck. The work is currently under development.


## Design
The design of this hash map is based in the reasoning of a state machine, where a key-value pair is allowed to be in many valid states. To transfer from one state to another, the table requires an atomic [compare-and-swap] function to compare-and-swap pointers.
### State Machine
![][img]

Our design bases on the assumption that a key, once inserted, is never deleted directly from the table, but it can be cleaned up in a certian way.

To simplify things and make them easier to understand, let us start by assumming that we never need to resize the table. If it is the case, then only the `put_if_match` function can alter the states. If a key-value is inserted to the table, then the transition `{Empty, Empty} -> {Key, Empty} -> {Key, Value}` atomically occurs in the strict order, and only one thread can switch the state at a time (since we are using an atomic `compare-and-swap`.) If two threads are competing to update the table, say `Thread 1` just finishes the transition `{Empty, Empty} -> {Key, Empty}` and `Thread 2` suddenly shows up at `{Key, Empty}` and wants to update the same key, what would happen? This is quite simple: Assume that `Thread 1` wants to update the key-value slot to `{Key, Value_1}` and `Thread 2` wants to update it to `{Key, Value_2}`, they will both be competing to swap in their value. Suppose that `Thread 1` wins, then the transition `{Key, Empty} -> {Key, Value_1}` occurs. However, `Thread 2` will still be trying to update and finally do the transition `{Key, Value_1} -> {Key, Value2}`, which is a valid transition. For the `get` function, it will read at any current state and return the found value immediately.

[...more explanation on the way...]

## Current State of Development
This library is still in its early development stage, *and requires Rust nightly to build*. For next milestone and current outstanding issues, see [0.1.0-alpha](https://github.com/rlei/nonblockinghashmap/milestone/1)

## Setup & Run

To build the library:
```bash
$ cargo +nightly build [--release]
```

To run the example::
```bash
$ cargo +nightly run --example testmain
```


[Dr. Cliff Click's design]: https://www.youtube.com/watch?v=WYXgtXWejRM
[originally implemented in Java]: https://github.com/boundary/high-scale-lib/blob/master/src/main/java/org/cliffc/high_scale_lib/NonBlockingHashMap.java
[compare-and-swap]: http://en.wikipedia.org/wiki/Compare-and-swap
[img]: http://i.imgur.com/3VmE7Nl.jpg
