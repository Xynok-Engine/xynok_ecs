# 🗓️ August 6th, 2026 - 22:12
## Reflecting on ECS Architecture

It has been over a week since I started this repository. I’ve been focusing on the system and scheduler implementation, taking inspiration from libraries like [Ralith/hecs](https://github.com/Ralith/hecs) and [bevy_ecs](https://github.com/bevyengine/bevy/tree/main/crates/bevy_ecs). 

When I built my previous ECS version, I didn't pay much attention to how other libraries handled their architecture. After diving into `Bevy`'s source code, I realized there are some great patterns I want to incorporate into my own project.

## Implementing SystemReturnValue

The most interesting feature I’ve found in `Bevy` is `system-return-value`. I plan to integrate this into my system to solve two specific problems:

* **Conditional execution:** Some systems only need to run when a specific trigger condition is met.
* **Dependency management:** Systems often rely on each other, requiring a strict execution order.

In my previous version, I didn't have a mechanism for this. It made managing the output of these systems quite difficult, especially when trying to keep the code generic. By adding `system-return-value`, I hope to make the system pipeline cleaner and much easier to reason about.
