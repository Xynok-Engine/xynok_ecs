# xynok_ecs
[![discord invite link](https://img.shields.io/discord/1495504680711880714?logo=discord)](https://discord.gg/a2qzfrFzWT)

## Features
- [x] register archetype
- [x] create/destroy entity
- [x] add/remove component
- [x] drop chunk data
- [x] tuple archetypes
- [ ] merge chunk/archetype layout
- [ ] query read/write

## Examples
**Cmd:** 
```bash
cargo run --example <name_of_rust_file_in_examples_folder>
```
**Example:** This command runs the file `examples/archetype.rs`.
```bash
cargo run --example archetype
```

## Concepts
To understand how the entire codebase works, you can check out these videos. 
I created them when I first started this repo. 
They explain the core concepts and the most important ideas that the codebase implements. 
I might update them in the future, but for now, they are a close match to the current state of the code.

| title | link |
| -------------- | --------------- |
| #0 Archetype ECS Explained: Chunk Storage & Memory Layout | ![youtube link](https://youtu.be/FT-aXDIjDUU) |
| #1 ECS Entities Explained: Packing ID + Version into One U64 | ![youtube link](https://youtu.be/rAe-KCWtnhk) |
| #2 Building an ECS World: Slot Tables, Archetypes & Versioning | ![youtube link](https://youtu.be/xa1Jea7789I) |
| #3 Add, Merge, Remove - The 3 ECS Component Ops | ![youtube link](https://youtu.be/ANyV3GwRIeU) |

