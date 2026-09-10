---
title: Xynok Ecs Overview
excerpt: An overview of the features and architecture of Xynok ECS
cover img: "../images/xynok_ecs_overview.png"
tags:
  - tutorial
  - ecs
---
## Overview

Xynok ECS is a lean library designed to provide a balanced feature set rather than attempting to be an all-encompassing ECS solution. Its primary goal is to offer a fundamental, straightforward, and effective API for managing data and logic within your application.

If you are looking for an ECS library with extensive documentation and a large community, I highly recommend [Bevy](https://github.com/bevyengine/bevy). However, if you want to understand how ECS works from the ground up by exploring the source code yourself, Xynok ECS is a great choice. Let's dive in.

## Architecture

Xynok ECS is built on a chunk-based architecture. Each chunk has a fixed size that aligns with the L3 cache of the CPU. Because each chunk contains a fixed amount of component data, iterating through a chunk allows you to access the complete data set for an entity efficiently.

## Component Types

Xynok ECS supports two types of components:

- **Normal components:** Standard data components attached to individual entities.
- **Shared components:** Components where all entities within a specific Archetype reference the same data instance.

## Modifiers

Xynok ECS provides three primary APIs for modifying the components of an entity. Whenever you modify an entity, it is moved to the Archetype that matches its new component layout:

- **Add:** Adds a component or a set of components to an entity.
- **Insert:** Adds a component or a set of components to an entity. If any of these components already exist, the library drops the old values and replaces them with the new ones.
- **Remove:** Removes a component or a set of components from an entity.
### Query
```rust
```
