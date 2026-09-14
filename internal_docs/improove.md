# Review: bug, hiệu năng và khả năng mở rộng

Ghi chép từ một lượt đọc toàn bộ core (`world`, `chunk`, `archetype`, `query`, `schedule`,
`collection`) tại commit `d68a7e1`, phiên bản `0.2.11`. Test suite lúc review là xanh.

Mọi con số trong tài liệu này đều đã được kiểm chứng bằng cách chạy thử, không phải ước đoán.

---

## 1. Bug thật, nên sửa trước

### 1.1. Archetype có component trùng type sẽ panic với thông báo vô nghĩa - ĐÃ SỬA

`src/chunk/layout.rs:112`

```rust
let state_offset = params.state_offsets_temp.remove(&des.storage_type_id).unwrap();
```

`Header::new` build `state_offsets` bằng `HashMap<TypeId, _>`, nên hai component cùng
`StorageType` chỉ để lại một entry. Lần `remove` thứ hai trả `None` rồi `unwrap()` nổ.
Ngoài ra `bytes_per_entity` trong `compute_layout` cũng đếm trùng nên `max_len` tính ra sai luôn.

Tệ hơn cái panic: khi tập component sau `normalize_set` trùng với một archetype đã có
(ví dụ `(Hp, Hp)` gặp world đã có archetype `Hp`), `create_archetype_id` đi vào nhánh tái sử
dụng và không dựng layout mới, nên không panic gì cả. Giá trị `Hp` thứ hai ghi đè lên giá trị
thứ nhất mà không drop, tức là rò bộ nhớ im lặng.

**Đã sửa như sau:**

- Thêm `ComponentDescriptor::fn_name` (`src/apis/mod.rs`, `src/apis/custom_type.rs`) để thông
  báo lỗi gọi được tên component. Phải là con trỏ hàm chứ không phải `&'static str`, vì
  `std::any::type_name` chưa const-stable mà `COMPONENT_DESCRIPTOR` lại là associated const.
  Lấy tên qua `ComponentDescriptor::name()`, không tốn gì cho tới khi thật sự cần in ra.
- Thêm `XynokEcsError::DuplicateComponentInArchetype(&'static str)`.
- **Chốt chặn chính:** `check_no_duplicate_component::<T>()` trong `src/world/mod.rs`, gọi từ
  `create_archetype_id`. Đây là đường duy nhất mọi `T: TArchetype` đi qua, và chỉ chạy đúng một
  lần cho mỗi `T` vì sau đó `archetype_counter` trả lời thẳng, nên hot path không tốn gì. Chỗ
  này bắt được cả trường hợp tái sử dụng archetype nói trên, thứ mà kiểm tra ở tầng layout không
  thể thấy.
- **Lớp phòng thủ thứ hai:** `build_component_bit_set` (đổi tên từ `build_component_but_set`)
  trong `src/chunk/layout.rs` báo lỗi khi một component đã có bit. Lớp này bọc các layout dựng
  bằng cách merge hai archetype.
- Bỏ `.unwrap()` ở `try_layout`, trả lỗi có tên component thay vì panic trống.

Kết quả:

```
thread 'main' panicked at examples/_dup_probe.rs:6:7:
Create Archetype `(Hp, Hp)` Failed: Archetype declares component `Hp` more than once.
A chunk keeps one column per component, so the second value would overwrite the first
without dropping it. Name each component once.
```

Nhờ `#[track_caller]` mà panic trỏ vào call site của người dùng chứ không phải `layout.rs:112`.

Test đi kèm: `duplicate_component_is_rejected` (`src/chunk/layout.rs`), cùng ba test trong
`tests/misc.rs` phủ ba đường vào khác nhau (`create`, `register_archetype`, và trường hợp tái
sử dụng archetype có sẵn).

### 1.2. Entity version bị clamp im lặng, mở đường cho ABA - ĐÃ SỬA

`src/entity.rs:50` và `World::new_entity` trong `src/world/mod.rs`

```rust
let version = version.clamp(Self::INITIALIZE_VERSION, Self::MAX_VERSION);
```

Một handle là cặp `(idx, version)`. `idx` là vị trí slot trong `World::entities`, `version` là
"đây là lần dùng thứ mấy của slot đó", và nó là thứ duy nhất phân biệt entity cũ với entity mới
nằm cùng chỗ. `new_entity` cấp `old_slot.version() + 1`, nhưng version chỉ có 24 bit và
`Entity::new` lại clamp im lặng.

Nên ở lần tái sử dụng thứ 2^24, `MAX_VERSION + 1` bị clamp ngược về đúng `MAX_VERSION`. Entity
mới có handle giống hệt entity vừa destroy. Từ đó mọi handle cũ đều pass `exists()`, và
`destroy(handle_cũ)` sẽ xóa nhầm entity đang sống.

16 triệu lần recycle trên cùng một slot là con số hoàn toàn đạt được với bullet hoặc particle.

**Đã sửa như sau:**

- `Entity::new` **báo lỗi** `EntityVersionOverflow` khi version vượt trần, thay vì clamp. Đầu
  dưới vẫn nâng lên `INITIALIZE_VERSION` vì đó là chuyện khác: version `0` thuộc về `NULL`, nên
  handle sống bắt đầu từ `1`.
- `World::erase_entity` **retire slot** khi nó hết version: slot vẫn nằm trong `entities` nhưng
  không được đẩy vào `free_entities` nữa, nên `create` sau sẽ lấy slot mới ở cuối `Vec`. Đúng
  một câu `match`.
- `World::new_entity` thêm `debug_assert!` để bắt sớm nếu có đường nào lỡ đẩy một slot đã cạn
  version vào lại hàng chờ.
- `World::retired_entity_slot_count()` công khai, để chỗ rò này quan sát được thay vì im lặng.
  Số này leo lên đều thường là dấu hiệu vài slot đang bị quay vòng trong một create/destroy
  loop chật, lúc đó pool entity sẽ hợp lý hơn là respawn.

**Giá phải trả:** mỗi slot bị retire rò một `EntitySpec` (40 byte), và chỉ sau 2^24 lần recycle
riêng slot đó. Đổi một cái rò có chặn trên và quan sát được, lấy một lỗ hổng tính đúng đắn
không có chặn trên. Bevy và EnTT chọn hướng ngược lại: để version wrap rồi ghi vào docs là
"đừng giữ handle lâu".

**Test:** `force_entity_version` trong `src/world/testing.rs` (test-util, hàm duy nhất trong
module đó có ghi) đẩy thẳng slot tới sát trần, vì không test nào chạy nổi 2^24 vòng
create/destroy thật. Ba test trong `tests/create_destroy.rs` phủ: slot cạn version bị retire,
slot còn version vẫn recycle như cũ, và một slot cạn không kéo theo các slot khác. Hai test
trong `src/entity.rs` phủ hai đầu của dải version.

Đã kiểm chứng ngược: tạm hoàn nguyên phần sửa thì hai trong ba test integration đỏ đúng chỗ.

### 1.3. `Chunk::new` không kiểm tra alloc trả null - ĐÃ SỬA

`src/chunk/mod.rs`, hàm `Chunk::new`

Trước đây con trỏ từ `std::alloc::alloc` được đem đi `write_bytes` luôn, không check null.
Hết bộ nhớ thì alloc trả null và cả chunk ghi thẳng vào địa chỉ 0.

Bản sửa thêm guard ngay sau lời gọi alloc:

```rust
let ptr = unsafe { std::alloc::alloc(layout.alloc_layout) };
if ptr.is_null()
{
    std::alloc::handle_alloc_error(layout.alloc_layout);
}
```

`handle_alloc_error` là cách xử lý chuẩn của Rust cho OOM, nó abort chứ không unwind, nên
không có đường nào để lộ ra một `Chunk` cầm con trỏ null.

Chỗ bất nhất giữa `alloc` và `dispose` cũng chốt lại luôn: `alloc_layout` luôn được dựng bằng
`Layout::from_size_align(CHUNK_SIZE_IN_BYTE, max_align)` trong `src/chunk/layout.rs`, mà
`CHUNK_SIZE_IN_BYTE` là hằng 16KB, nên size không bao giờ bằng 0. Guard
`if alloc_layout.size() != 0` trong `dispose` là code chết, đã bỏ đi. Nếu size 0 thật sự xảy ra
được thì bản thân lời gọi `alloc` bên `new` đã là UB từ đầu rồi, guard bên `dispose` cũng không
cứu được gì.

Không thêm test cho phần này: ép alloc trả null một cách tin cậy thì phải thay global allocator,
mà nhánh đó kết thúc bằng abort nên test cũng chỉ quan sát được tiến trình chết. Toàn bộ 54 unit
test và các test integration vẫn xanh sau khi sửa.

### 1.4. `&mut World` bị alias giữa các thread trong parallel group - ĐÃ SỬA

`src/schedule/scheduler.rs`, hàm `run_system_group`

`HeapMut<World>` được copy vào mọi closure. Mỗi job gọi `Query::new(world, ..)` rồi vào
`get_or_create_query_src_access(&mut self, ..)`. Nhiều thread cùng tạo `&mut World` một lúc là
UB theo aliasing rules của Rust, kể cả khi trên thực tế không có ai ghi (pass `prepare` đã
refresh version rồi nên nhánh `query_spec.archetypes.clear()` không chạy).

**Đã sửa:** tách `get_or_create_query_src_access` thành hai nửa trong `src/world/mod.rs`:

- `prepare_query_src_access::<T>(&mut self) -> Result<usize, _>` giữ toàn bộ phần ghi, tức là
  đăng ký `QuerySpec` và dựng lại danh sách archetype khi version lệch.
- `query_src_access::<T>(&self, last_run_tick) -> Option<QuerySpecAccessor>` là đường đọc thuần.
  Trả `None` khi query chưa đăng ký hoặc spec đã cũ, để caller biết phải quay về nhánh `&mut`.

`get_or_create_query_src_access` giờ chỉ là `prepare` rồi `query_src_access`, nên đường đơn
luồng (`World::create_query`, system chạy một mình) không đổi hành vi.

Phía system param, `TSystemParam` có thêm `fn prepare(world, last_run_tick)`
(`src/system/traits.rs`). Macro sinh system gọi `$name::prepare(..)` trong `TSystem::prepare`
thay vì gọi `init` rồi vứt kết quả đi. Trong `Query::init`, đường đọc
`Query::new_prepared(&World, ..)` được thử trước; chỉ khi nó trả `None` mới rơi xuống
`Query::new(&mut World, ..)`. Sau pass `prepare` của scheduler thì mọi job trong parallel group
đều đi nhánh đọc, không job nào dựng `&mut World` nữa.

### 1.5. `Enable<T>` / `Disable<T>` ghi sai kiểu vào column - ĐÃ SỬA

`src/wrapper/mod.rs`

`COMPONENT_DESCRIPTOR` lấy `byte_size`, `align` và `fn_drop` theo `T::StorageType`, nhưng
`Chunk::write_at::<Enable<Hp>>` lại làm `(col_ptr as *mut Enable<Hp>).write(value)`.
`Enable<T>` là `struct { pub val: T }` với `repr(Rust)`, nghĩa là layout không có bảo đảm nào.
Hiện tại chạy đúng vì compiler đặt field ở offset 0, nhưng đó là may chứ không phải bảo đảm.

**Đã sửa:** thêm `#[repr(transparent)]` vào struct sinh ra bởi macro `define_enable!`.
Wrapper giờ được bảo đảm cùng layout với `T`, nên `(col_ptr as *mut Enable<Hp>).write(value)`
khớp với layout mà `COMPONENT_DESCRIPTOR` đã dùng để cấp phát column.

---

## 2. Hiệu năng

### 2.1. `compute_layout` dò tuyến tính, trừ 1 mỗi vòng - ĐÃ SỬA

`src/chunk/layout.rs`, hàm `compute_layout`

Ước lượng ban đầu chỉ tính `byte_size * 8 + 1 bit enable`, bỏ qua hoàn toàn vùng `added` và
`changed` (4 + 4 byte mỗi entity mỗi component `ChangeAble`) cùng với padding. Kết quả là
`max_entities` khởi điểm thừa rất nhiều, rồi `loop { max_entities -= 1 }` gọi lại `try_layout`.
Mỗi lượt `try_layout` đều `clear` và insert HashMap cho từng component, `Header::new` bên trong
cũng dựng HashMap nữa.

Số vòng lãng phí đo được:

| Archetype | Ước lượng đầu | Thực tế | Số vòng thừa |
|---|---|---|---|
| 1 x `ChangeAble` u32 | 1351 | 819 | 532 |
| 4 x `ChangeAble` u32 | 668 | 292 | 376 |
| 8 x `ChangeAble` u64 | 224 | 119 | 105 |

Với một game có vài trăm archetype thì đây là hàng trăm nghìn lượt thao tác HashMap lúc khởi động.

**Đã sửa như sau,** làm cả hai hướng đã đề xuất:

- Tách phần ước lượng ra thành `estimate_max_entities`, và cho nó cộng đúng chi phí state:
  1 bit cho `EnableAble`, `CHANGED_TICK_BYTE_SIZE * 2` cho `ChangeAble`, đọc thẳng từ
  `state_detection` của từng component thay vì cộng đại 1 bit cho mọi component như trước.
- Đổi vòng lặp thành binary search trên `max_entities`. `try_layout` monotonic (mọi phần của
  layout đều lớn lên theo số dòng), nên đã vừa ở một cỡ thì mọi cỡ nhỏ hơn cũng vừa.

Một chi tiết nhỏ đi kèm: chỉ `ArchetypeIsTooLarge` mới thu hẹp khoảng tìm kiếm. Các lỗi khác
(component trùng, `Layout::from_size_align` hỏng) nói cùng một chuyện ở mọi số dòng, nên trả
thẳng ra ngoài thay vì bị nuốt rồi biến thành `ArchetypeIsTooLarge` như vòng lặp cũ.

Ước lượng mới đo lại:

| Archetype | Ước lượng đầu | Thực tế | Số lượt `try_layout` |
|---|---|---|---|
| 1 x `ChangeAble` u32 | 819 | 818 | 10 (trước: 533) |
| 4 x `ChangeAble` u32 | 292 | 292 | 9 (trước: 377) |

**Test:** `estimate_is_a_tight_upper_bound` (`src/chunk/layout.rs`) khoá lại cả hai tính chất mà
binary search dựa vào: ước lượng không được thấp hơn sức chứa thật (search sẽ chọn hụt), và
không được vượt quá 5% (search phải trả tiền cho phần thừa). Ba archetype phủ có tracking,
không tracking, và trộn cả hai. Các test layout cũ vẫn xanh, trong đó `uses_the_chunk_efficiently`
canh mức lấp đầy chunk từ 90% trở lên.

### 2.2. Không có edge cache cho add/remove component - ĐÃ SỬA

`src/world/mod.rs`, các hàm `add_component`, `merge_component`, `remove_component`

Mỗi lần gọi đều phải làm lại từ đầu:

1. Duyệt `component_col_descriptors` (HashMap) của arch A và arch B, mỗi key lại tra
   `component_counter.index_of`
2. `sort` và `dedup`
3. Hash một `Vec<usize>` để tra `component_set_counter`

Toàn bộ khối này cho ra cùng một kết quả với cùng cặp `(a_arch_id, T)`, nên cache lại là ăn ngay.

**Đã sửa:** `World` có thêm hai bảng `add_edges` và `remove_edges`, khóa bằng struct
`ArchetypeEdgeKey { src_arch_id, archetype_type_id }` (`src/world/arch_spec.rs`). Giá trị là
`ArchetypeEdge { dst_arch_id, migration }`, tức vừa là id archetype đích vừa là migration plan ở
mục 2.3. Hai bảng riêng vì thêm cho ra hợp còn bớt cho ra hiệu của hai tập component.

Lần đầu với một cặp `(src, T)` vẫn làm đủ việc cũ, từ lần thứ hai chỉ còn đúng một lượt hash.
Cache không bao giờ phải dọn: layout bất biến sau khi tạo, còn registry archetype chỉ mọc thêm.

`b_arch_id` giờ chỉ còn debug build cần, để giữ mấy `panic!` báo dùng sai API. Release đi thẳng
qua edge cache.

Nhân tiện, field `ArchetypeSpec::archetypes: HashMap<usize, Archetype>` luôn rỗng và không ai đọc
đã được xóa. Chỗ dành sẵn cho edge cache thì edge cache đã nằm trong `World` rồi.

### 2.3. `take_from` và `swap_remove_at` duyệt HashMap trong đường nóng - ĐÃ SỬA

`src/chunk/mod.rs`

Cả hai đều `for (k, des) in layout.component_col_descriptors.iter()` rồi
`component_specs.get(k).unwrap()` và `dst_layout.component_col_descriptors.get(k)` cho từng
component. Tức là 3 lượt hash mỗi component mỗi entity được di chuyển.

**Đã sửa, hai phần:**

Một, `ChunkLayout` có thêm `columns: Vec<ColumnEntry>` dạng dense
(`ColumnEntry { storage_type_id, offset, byte_size, state_offset, fn_drop }` trong
`src/chunk/column.rs`). Mọi thứ cần để đụng vào một column đều nằm sẵn trong đó, nên
`swap_remove_at` và `dispose` chỉ duyệt slice, không hash và cũng không cần `ComponentSpecs` nữa.
HashMap `component_col_descriptors` vẫn còn, nhưng chỉ để tra theo `TypeId` lúc setup.

Hai, `src/chunk/migration.rs` tính sẵn migration plan cho mỗi cặp archetype nguồn, đích:

```rust
pub enum ComponentMigrationAction
{
    Move,        // bên đích cũng có column này
    DropInPlace, // caller sắp ghi đè, giá trị cũ phải drop chứ không mang sang
    Abandon,     // caller đã lấy giá trị ra rồi, chỉ dồn hàng cuối xuống
}

pub struct ComponentMigration
{
    pub src_offset:       usize,
    pub dst_offset:       usize,
    pub byte_size:        usize,
    pub fn_drop:          FnComponentDropItSelf,
    pub src_state_offset: StateOffset,
    pub dst_state_offset: StateOffset,
    pub action:           ComponentMigrationAction,
}

pub struct MigrationPlan
{
    pub components: Vec<ComponentMigration>,
}
```

Plan nằm luôn trong `ArchetypeEdge` ở mục 2.2. `take_from` giờ là một vòng lặp trên slice với một
`match` trên `action`, không hash gì cả, và `ChunkTakeComponentParams` bỏ luôn hai field
`component_specs` với `overwritten_type_ids` vì việc phân loại đã chốt từ lúc dựng plan.

### 2.4. Memory không bao giờ được thu hồi

- `Archetype::chunks` chỉ push, không bao giờ dealloc chunk rỗng. Peak 1 triệu entity thì
  16KB nhân N chunk giữ nguyên cho tới khi drop `World`.
- `World::entities` cũng chỉ grow.
- `free_chunks` (Queue) và `free_chunks_stored` (HashSet) giữ cùng một tập, tốn gấp đôi bộ nhớ
  và hai lần cập nhật.

**Hướng sửa:**

- Thay cặp Queue + HashSet bằng một `Vec<usize>` dùng như stack, cộng một cờ `is_cached: bool`
  ngay trong `Chunk`. Gọn hơn và còn lợi về locality vì tái dùng chunk vừa động vào.
- Thêm API kiểu `World::shrink_to_fit()`, hoặc tự động trả chunk về allocator khi một archetype
  có quá N chunk rỗng.

### 2.5. `retain` lồng trong vòng lặp

`src/world/mod.rs`, hàm `retain_archetype_component_id_of_to`

`component_set.retain(...)` nằm trong vòng lặp qua component của B, thành O(n*m) với n lần dịch
chuyển Vec. Số component nhỏ nên hiện tại không sao.

**Hướng sửa:** nếu đi theo hướng 2.2 thì chuyển hẳn `component_set_counter` sang khóa bằng
`ComponentBitSet` (đã có sẵn trong `src/collection/`) thay vì `Vec<usize>` phải sort rồi hash.

---

## 3. Thiếu để mở rộng

### 3.1. `cmd_buffer` rỗng, system không thể tạo hoặc hủy entity

`src/cmd_buffer/mod.rs` đang 0 dòng. Hệ quả là system chỉ đọc và ghi được component có sẵn:
không spawn, không despawn, không add hay remove component.

Đây là thứ chặn nhiều nhất khả năng dùng thật, và cũng là điều kiện để parallel group có ý
nghĩa, vì structural change phải được hoãn tới cuối group rồi apply trên main thread.

### 3.2. Thread pool cố định 4 thread

`src/schedule/scheduler.rs`

```rust
ThreadPool::new(CfgThreadPool::new("Default Xynok ECS Scheduler ThreadPool", 4))
```

**Hướng sửa:** lấy mặc định từ `std::thread::available_parallelism()` và cho phép cấu hình qua
một `SchedulerConfig`, vì `DefaultScheduler::new` hiện chỉ nhận mỗi `world`.

### 3.3. `CHUNK_SIZE_IN_BYTE` hardcode 16KB

`src/apis/constants.rs`

Archetype có một component lớn hơn 16KB là `ArchetypeIsTooLarge` luôn, không có đường thoát.

**Hướng sửa:** nếu muốn hỗ trợ component to (mesh data, ma trận skinning) thì cần cho phép chunk
size theo từng archetype, hoặc ít nhất tự nới lên bội số của 16KB khi `max_entities` tính ra 0.

### 3.4. Hai bộ đếm id phải tự đồng bộ với nhau

`archetype_id` được cấp bằng `component_set_counter.len()` ở ba chỗ khác nhau trong
`src/world/mod.rs`, trong khi `build_archetype_which_contains` lại giả định index trong
`ArchetypeSpecs` trùng với arch_id. Hiện tại đúng vì mọi đường cấp id đều insert đúng một entry
vào cả hai, nhưng đây là loại bất biến rất dễ vỡ khi thêm một nhánh mới.

**Hướng sửa:** cho `ArchetypeSpecs.len()` làm nguồn sự thật duy nhất.

---

## 4. Chi tiết nhỏ

- **Lifetime không ràng buộc** trong `Chunk::get_component<'a>(&self) -> Result<&'a C>` và
  `get_components_mut<'a>(&mut self) -> Result<&'a mut [C]>` (`src/chunk/mod.rs`). `Chunk` là
  crate-private nên chưa lộ ra ngoài, nhưng đây là mìn: code mới trong crate có thể vô tình giữ
  `&mut [C]` sau khi chunk đã bị move. Nên đổi về lifetime bình thường gắn với `&self`, chỗ nào
  thật sự cần thoát ra thì dùng raw pointer cho rõ ý đồ.
- **`World::exists` nhận `&mut self`** mà chỉ đọc (`src/world/mod.rs`). Đổi sang `&self` để gọi
  được khi đang giữ shared borrow.
- **`destroy` và `add_component` chỉ có `debug_assert!(self.exists(e))`.** Ở release, thao tác
  với handle cũ sẽ âm thầm sửa nhầm entity khác. Nên có thêm bản `try_destroy() -> Result` và
  giữ `destroy` panic ở cả release, hoặc ít nhất document rõ đây là unchecked.
- **`Query::with_shared_component_filter`** (`src/query/mod.rs`) là stub rỗng đang để public.
  Nên `#[doc(hidden)]` hoặc bỏ hẳn cho tới khi làm thật.
- **Rò rỉ ở error path:** trong `Archetype::take_and_write_from`, nếu `T::write_at` fail sau khi
  `chunk.take_from` đã chạy thì row nguồn đã bị move ra mà `len` hai bên chưa cập nhật, world
  kẹt ở trạng thái hỏng. Hiện caller luôn `panic!` trên `Err` nên chưa lộ, nhưng nếu sau này đổi
  sang trả `Result` ra ngoài thì phải xử lý cho tử tế.

---

## Thứ tự làm đề xuất

1. ~~**Mục 1.1**~~ đã xong. **Mục 1.3, 1.5:** nhỏ, độc lập, sửa là xong.
2. **Mục 2.1:** sửa công thức ước lượng cộng binary search. Ăn ngay vài trăm vòng lặp mỗi archetype.
3. ~~**Mục 1.2:** quyết định chính sách retire slot.~~ đã xong.
4. **Mục 2.2 và 2.3:** edge cache và migration plan đi chung với nhau. Đây là phần thay đổi lớn
   nhất nhưng cũng đáng nhất về hiệu năng.
5. **Mục 1.4 và 3.1:** làm cùng lúc, vì command buffer sẽ định hình lại chữ ký
   `&World` / `&mut World` của scheduler.

Gom (1.1, 1.3, 1.5, 2.1) thành một đợt là hợp lý, vì đều gọn và không đụng vào nhau.
