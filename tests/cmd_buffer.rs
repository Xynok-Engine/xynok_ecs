//! Cmd buffer: mọi thao tác ghi qua `Cmd` chỉ là một bản ghi, không đụng gì tới world cho tới
//! khi `World::sync_point()` chạy. Bộ test này bám vào hai thứ: lúc nào thay đổi mới hiện ra,
//! và sau khi flush thì trạng thái có đúng như gọi thẳng API của `World` hay không.
//!
//! System trong scheduler là `Fn` chứ không phải `FnMut`, nên không capture được biến local.
//! Các test vì vậy trao đổi dữ liệu với system qua static. Test runner chạy các test song song
//! trong cùng một binary, nên mỗi test giữ static riêng và không dùng chung với test khác.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use xynok_ecs::cmd_buffer::cmd::Cmd;
use xynok_ecs::component;
use xynok_ecs::entity::Entity;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::{testing, World};
use xynok_std::unsafe_ptr::HeapPtr;

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mana(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Armor(u32);

const UPDATE: DefaultScheduleSession = DefaultScheduleSession::Update;

/// Chỗ để system và test đưa entity qua lại cho nhau.
///
/// `lock` ở đây bỏ qua poison: một test fail không nên kéo test khác fail lây.
struct Slot(Mutex<Vec<Entity>>);

impl Slot
{
    const fn new() -> Self
    {
        Self(Mutex::new(Vec::new()))
    }
    fn push(&self, e: Entity)
    {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push(e);
    }
    fn set(&self, e: Entity)
    {
        let mut g = self.0.lock().unwrap_or_else(|p| p.into_inner());
        g.clear();
        g.push(e);
    }
    fn all(&self) -> Vec<Entity>
    {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn one(&self) -> Entity
    {
        self.all()[0]
    }
}

/// World mới toanh, đặt trên heap vì scheduler giữ con trỏ tới nó.
fn new_world() -> HeapPtr<World>
{
    HeapPtr::new(World::default())
}

// ------------------------------------------------------------------------------------------------
// create
// ------------------------------------------------------------------------------------------------

static CREATE_ONE: Slot = Slot::new();

fn sys_create_one(mut cmd: Cmd)
{
    CREATE_ONE.push(cmd.create(Hp(10)));
}

#[test]
fn t_create_chi_hien_ra_sau_sync_point()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_one);
    scheduler.run(UPDATE);

    let e = CREATE_ONE.one();

    // Handle đã nằm trong tay ngay lúc gọi, nhưng entity thì chưa thuộc archetype nào.
    assert!(!world.exists(e), "{e} không được phép tồn tại trước sync_point");
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0, "query không được thấy gì trước khi flush");

    world.sync_point();

    assert!(world.exists(e), "{e} phải sống sau sync_point");
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(10));
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// Đủ lớn để xài hết lô pre-allocate 32 cái rồi phải xin thêm lô nữa.
const MANY: u32 = 100;
static CREATE_MANY: Slot = Slot::new();

fn sys_create_many(mut cmd: Cmd)
{
    for i in 0..MANY
    {
        CREATE_MANY.push(cmd.create(Hp(i)));
    }
}

#[test]
fn t_create_nhieu_hon_mot_lo_pre_allocate()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_many);
    scheduler.run(UPDATE);
    world.sync_point();

    let created = CREATE_MANY.all();
    assert_eq!(created.len(), MANY as usize);

    // Không có handle nào bị phát trùng, kể cả ở chỗ nối giữa hai lô pre-allocate.
    let mut slots: Vec<_> = created.iter().map(|e| e.idx()).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), MANY as usize, "cmd buffer phát trùng entity giữa các lô pre-allocate");

    for (i, &e) in created.iter().enumerate()
    {
        assert!(world.exists(e), "{e} phải sống sau sync_point");
        assert_eq!(testing::read_component::<Hp>(&world, e), Hp(i as u32));
    }

    // Phần entity pre-allocate còn thừa vẫn chưa gắn vào archetype nào, nên query không thấy.
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), MANY as usize);
}

// ------------------------------------------------------------------------------------------------
// add / merge / remove component
// ------------------------------------------------------------------------------------------------

static ADD_TARGET: Slot = Slot::new();

fn sys_add_component(mut cmd: Cmd)
{
    cmd.add_component(ADD_TARGET.one(), Mana(7));
}

#[test]
fn t_add_component_deferred()
{
    let mut world = new_world();
    let e = world.create(Hp(1));
    ADD_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_add_component);
    scheduler.run(UPDATE);

    assert_eq!(
        world.create_query::<&Mana>().into_iter().count(),
        0,
        "Mana không được xuất hiện trước sync_point"
    );

    world.sync_point();

    assert_eq!(testing::read_component::<Mana>(&world, e), Mana(7));
    assert_eq!(
        testing::read_component::<Hp>(&world, e),
        Hp(1),
        "component cũ phải theo entity sang archetype mới"
    );
}

static MERGE_TARGET: Slot = Slot::new();

fn sys_merge_component(mut cmd: Cmd)
{
    // merge vừa ghi đè component đã có, vừa thêm component chưa có.
    cmd.merge_component(MERGE_TARGET.one(), (Hp(99), Armor(5)));
}

#[test]
fn t_merge_component_deferred()
{
    let mut world = new_world();
    let e = world.create(Hp(1));
    MERGE_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_merge_component);
    scheduler.run(UPDATE);

    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(1), "giá trị cũ phải giữ nguyên trước sync_point");

    world.sync_point();

    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(99));
    assert_eq!(testing::read_component::<Armor>(&world, e), Armor(5));
}

static REMOVE_TARGET: Slot = Slot::new();

fn sys_remove_component(mut cmd: Cmd)
{
    cmd.remove_component::<Mana>(REMOVE_TARGET.one());
}

#[test]
fn t_remove_component_deferred()
{
    let mut world = new_world();
    let e = world.create((Hp(1), Mana(2)));
    REMOVE_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_remove_component);
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1, "Mana phải còn nguyên trước sync_point");

    world.sync_point();

    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0);
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(1));
    assert!(world.exists(e));
}

// ------------------------------------------------------------------------------------------------
// destroy
// ------------------------------------------------------------------------------------------------

static DESTROY_TARGET: Slot = Slot::new();

fn sys_destroy(mut cmd: Cmd)
{
    cmd.destroy(DESTROY_TARGET.one());
}

#[test]
fn t_destroy_deferred()
{
    let mut world = new_world();
    let keep = world.create(Hp(1));
    let doomed = world.create(Hp(2));
    DESTROY_TARGET.set(doomed);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_destroy);
    scheduler.run(UPDATE);

    assert!(world.exists(doomed), "entity phải còn sống tới lúc flush");

    world.sync_point();

    assert!(!world.exists(doomed));
    assert!(world.exists(keep));
    assert_eq!(
        testing::read_component::<Hp>(&world, keep),
        Hp(1),
        "swap-remove không được làm hỏng hàng còn lại"
    );
    assert_eq!(testing::entity_stored_at_row_of(&world, keep), keep);
}

// ------------------------------------------------------------------------------------------------
// thứ tự thực thi
// ------------------------------------------------------------------------------------------------

static ORDERED: Slot = Slot::new();

fn sys_create_then_touch(mut cmd: Cmd)
{
    // create -> add -> merge -> add -> remove trên cùng một entity, tất cả đều chỉ là bản ghi.
    // Chạy sai thứ tự thì add_component sẽ nổ ngay vì entity chưa tồn tại.
    let e = cmd.create(Hp(1));
    cmd.add_component(e, Mana(2));
    cmd.merge_component(e, Hp(3));
    cmd.add_component(e, Armor(4));
    cmd.remove_component::<Mana>(e);
    ORDERED.push(e);
}

#[test]
fn t_lenh_chay_dung_thu_tu_ghi()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_then_touch);
    scheduler.run(UPDATE);
    world.sync_point();

    let e = ORDERED.one();
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(3), "merge phải chạy sau create");
    assert_eq!(testing::read_component::<Armor>(&world, e), Armor(4));
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0, "remove phải chạy sau add");
}

static CREATE_DESTROY: Slot = Slot::new();

fn sys_create_then_destroy(mut cmd: Cmd)
{
    let keep = cmd.create(Hp(1));
    let doomed = cmd.create(Hp(2));
    cmd.destroy(doomed);
    CREATE_DESTROY.push(keep);
    CREATE_DESTROY.push(doomed);
}

#[test]
fn t_create_roi_destroy_trong_cung_mot_lo()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_then_destroy);
    scheduler.run(UPDATE);
    world.sync_point();

    let out = CREATE_DESTROY.all();
    assert!(world.exists(out[0]));
    assert!(!world.exists(out[1]), "entity vừa tạo rồi destroy trong cùng lô phải biến mất sau flush");
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// ------------------------------------------------------------------------------------------------
// vòng đời của buffer
// ------------------------------------------------------------------------------------------------

static FLUSH_TWICE: Slot = Slot::new();

fn sys_create_for_double_flush(mut cmd: Cmd)
{
    FLUSH_TWICE.push(cmd.create(Hp(1)));
}

#[test]
fn t_sync_point_lan_hai_khong_lam_gi_them()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_double_flush);
    scheduler.run(UPDATE);

    world.sync_point();
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);

    world.sync_point();
    world.sync_point();
    assert_eq!(
        world.create_query::<&Hp>().into_iter().count(),
        1,
        "flush lại buffer rỗng không được tạo thêm gì"
    );
}

static ACCUMULATE: Slot = Slot::new();

fn sys_create_for_accumulate(mut cmd: Cmd)
{
    ACCUMULATE.push(cmd.create(Hp(1)));
}

#[test]
fn t_nhieu_lan_run_don_lai_roi_flush_mot_the()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_accumulate);
    scheduler.run(UPDATE);
    scheduler.run(UPDATE);
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0);

    world.sync_point();

    assert_eq!(
        world.create_query::<&Hp>().into_iter().count(),
        3,
        "lệnh của nhiều lần run phải dồn lại chứ không mất"
    );
}

static INTERLEAVED: Slot = Slot::new();

fn sys_create_for_interleaved(mut cmd: Cmd)
{
    INTERLEAVED.push(cmd.create(Hp(1)));
}

#[test]
fn t_flush_xen_ke_giua_cac_lan_run()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_interleaved);

    for expected in 1..=3
    {
        scheduler.run(UPDATE);
        world.sync_point();
        assert_eq!(world.create_query::<&Hp>().into_iter().count(), expected);
    }
}

#[test]
fn t_sync_point_tren_world_chua_tung_dung_cmd()
{
    let mut world = World::default();
    world.create(Hp(1));
    // Chưa có worker nào, flush phải là no-op chứ không phải panic.
    world.sync_point();
    world.sync_point();
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// ------------------------------------------------------------------------------------------------
// nhiều worker
// ------------------------------------------------------------------------------------------------

const PER_WORKER: u32 = 50;
static WORKER_A: Slot = Slot::new();
static WORKER_B: Slot = Slot::new();

fn sys_worker_a(mut cmd: Cmd)
{
    for i in 0..PER_WORKER
    {
        WORKER_A.push(cmd.create(Hp(i)));
    }
}

fn sys_worker_b(mut cmd: Cmd)
{
    for i in 0..PER_WORKER
    {
        WORKER_B.push(cmd.create(Mana(i)));
    }
}

#[test]
fn t_hai_system_song_song_moi_ben_mot_buffer()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_worker_a, sys_worker_b));
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0);

    world.sync_point();

    let a = WORKER_A.all();
    let b = WORKER_B.all();
    assert_eq!(a.len(), PER_WORKER as usize);
    assert_eq!(b.len(), PER_WORKER as usize);

    // Hai worker xin entity từ cùng một allocator, không ai được nhận trùng slot của ai.
    let mut slots: Vec<_> = a.iter().chain(b.iter()).map(|e| e.idx()).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), 2 * PER_WORKER as usize, "hai worker nhận trùng entity slot");

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), PER_WORKER as usize);
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), PER_WORKER as usize);
    for &e in a.iter().chain(b.iter())
    {
        assert!(world.exists(e), "{e} do worker tạo phải sống sau sync_point");
    }
}

// ------------------------------------------------------------------------------------------------
// phối hợp với query
// ------------------------------------------------------------------------------------------------

static SPAWNED_FROM_QUERY: AtomicUsize = AtomicUsize::new(0);

fn sys_spawn_per_row(mut cmd: Cmd, query: Query<&Hp>)
{
    for hp in query
    {
        cmd.create(Mana(hp.0));
        SPAWNED_FROM_QUERY.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn t_tao_entity_trong_luc_dang_duyet_query()
{
    let mut world = new_world();
    for i in 0..10
    {
        world.create(Hp(i));
    }

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_spawn_per_row);
    scheduler.run(UPDATE);

    // Vì lệnh bị hoãn nên vòng lặp không tự nhân hàng lên khi đang chạy.
    assert_eq!(SPAWNED_FROM_QUERY.load(Ordering::SeqCst), 10);

    world.sync_point();
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 10);
}

static DESPAWN_COUNT: AtomicU32 = AtomicU32::new(0);

fn sys_destroy_even(mut cmd: Cmd, query: Query<(&Entity, &Hp)>)
{
    for (e, hp) in query
    {
        if hp.0 % 2 == 0
        {
            cmd.destroy(*e);
            DESPAWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn t_destroy_hang_loat_giu_nguyen_phan_con_lai()
{
    let mut world = new_world();
    let all: Vec<Entity> = (0..40).map(|i| world.create(Hp(i))).collect();

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_destroy_even);
    scheduler.run(UPDATE);
    world.sync_point();

    assert_eq!(DESPAWN_COUNT.load(Ordering::SeqCst), 20);
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 20);

    for (i, &e) in all.iter().enumerate()
    {
        let alive = i % 2 == 1;
        assert_eq!(world.exists(e), alive, "{e} (hp={i}) sai trạng thái sống chết sau flush");
        if alive
        {
            assert_eq!(testing::read_component::<Hp>(&world, e), Hp(i as u32));
            assert_eq!(testing::entity_stored_at_row_of(&world, e), e, "swap-remove làm lệch ánh xạ hàng của {e}");
        }
    }
}
