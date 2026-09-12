---
title: Xynok Ecs Overview
excerpt: An overview of the features and architecture of Xynok ECS
cover img: "../images/xynok_ecs_overview.png"
tags:
  - concept
---

## Overview
test new version docs

## Features
- [x] `add/merge/remove` components
- [x] Query: `Query<&T>, Query<&mut T>, Query<(&A, &B, &mut C,... up to 16 params)>`
- [x] Query states: 
    - [x] `Query<Disabled<&T>>, Query<Enabled<&mut T>>`
    - [x] `Query<(&A, &mut B, Disabled<&C>, Enabled<&D>, Changed<&E>, Added<&F>)>`]
- [x] system & scheduler:
    - [x] `fn system_a(enemies: Query<(&Enemy, &mut Position, &MoveSpeed)>, player: Query<(&Player, &Position)>)`
    - [x] single-thread system: `scheduler.add_system()`
    - [x] multi-thread systems: `scheduler.add_system_parallel((system_a, system_b, system_c, ... up to 16 params))`
