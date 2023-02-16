# SemiStr

## Intro

An immutable string stored inline or on heap.

It occuipies 16 bytes on stack.

Inline: 4-byte length + 12 bytes data. 
Heap: 4-byte length + 4-byte prefix data + 8-byte pointer to atomic reference counting data.

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   https://opensource.org/licenses/MIT)

at your option.
