---
title: Xynok Ecs Overview
excerpt: An overview of the features and architecture of Xynok ECS
cover img: "../images/xynok_ecs_overview.png"
tags:
  - tutorial
  - ecs
---

## Overview
Xynok ECS is a lean library that provides a balanced feature set instead of trying to be a feature-packed ECS solution.
The primary goal of Xynok ECS is to provide a fundamental, simple, and effective ECS API for your application to manage data and logic.

If you want a fully, major, massive documentation and large community ecs library in Rust, I highly recommend [Bevy](https://github.com/bevyengine/bevy).

Buf if you want to understand any piece of code and how ecs actually work from scratch, xynok ecs is a good option(theo góc nhìn của tôi). So, let go !

## Concepts
Xynok_ecs is built on Chunk base architecture. Each chunk has fixed size matching L3 of CPU hardware device. 
Each chunk contains a fixed amount of components data pack. That mean when you walk through a chunk, you can get fully data of an entity.

## Components
xynok_Ecs provides 2 type of components:
- normal component
- shared component: All entities in an Archetype will reuse same component.

## Features

### Modifers

xynok_ecs provides 3 api to let you edit components of an entity:

- add: you add an component/ or a set of component to an entity, this entity will be moved to another Archetype matching new layout
- add: you add an component/ or a set of component to an entity, this entity will be moved to another Archetype matching new layout. If any of these component already exist
in entity, it drop old ones and replace with new values.
- remove: remove a component/or a set of componts from entity,  this entity will be moved to another Archetype matching new layout

### Query
```rust
```
