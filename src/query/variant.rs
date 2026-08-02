#![allow(unused)]
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::query::src_access::SrcAccess;
use crate::world::query_spec::QuerySpecAccessor;
use std::any::TypeId;

pub trait TQueryParamBuilderSrcAccess<T: TComponent + 'static> {}

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;

    type SrcAccess = SrcAccess;

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope {
            read:    vec![TypeId::of::<T::StorageType>()],
            write:   vec![],
            exclude: vec![],
        })
    }

    fn build_src_access(src_access: &QuerySpecAccessor) -> Self::SrcAccess
    {
        let current_chunk_len = match src_access.len() > 0
        {
            true =>
            {
                let first_arch = unsafe { &**(src_access.as_mut_ptr()) };
                first_arch.arch.chunk_count()
            }
            false => 0usize,
        };
        SrcAccess {
            archetypes:        src_access.as_mut_ptr(),
            total_arch:        src_access.len(),
            current_arch_idx:  0,
            current_chunk_len: 0,
            current_chunk_idx: 0,
        }
    }

    fn next<'a>(src_access: &mut Self::SrcAccess) -> Option<Self::QueryItem<'a>>
    {
        let current_chunk_len = src_access.current_chunk_len;
        let current_chunk_len = src_access.current_chunk_len;
        let current_chunk_len = src_access.current_chunk_len;
        todo!()
    }
}
macro_rules! impl_query_param {
    ($target:ty, $query_check:ident) => {
        impl<T: TComponent + 'static> TQueryParam for $target
        {
            type QueryItem<'a> = &'a T::StorageType;
            type SrcAccess = SrcAccess;
            const TYPE_ID: &'static TypeId = &TypeId::of::<T>();
            fn access_scope() -> Result<AccessScope, XynokEcsError>
            {
                Ok(AccessScope {
                    read:    vec![TypeId::of::<T::StorageType>()],
                    write:   vec![],
                    exclude: vec![],
                })
            }
            fn build_src_access() -> Self::SrcAccess
            {
                todo!()
            }
            fn next<'a>(src_access: &mut Self::SrcAccess) -> Option<Self::QueryItem<'a>>
            {
                todo!()
            }
        }
    };
}
//impl_query_param!(&T, e);
macro_rules! impl_query_param_mut {
    ($target:ty, $query_check:ident) => {
        impl<T: TComponent + 'static> TQueryParam for $target
        {
            type QueryItem<'a> = &'a mut T;
            type Item<'a> = (Entity, &'a mut T);
            type SrcAccess = SrcAccessMut<T>;
            const ACCESS_TYPES: &'static [TypeId] = &[TypeId::of::<T>()];
            const EXCLUDE_TYPES: &'static [TypeId] = &[];
            fn access_scope() -> AccessScope
            {
                AccessScope {
                    write:   vec![TypeId::of::<T>()],
                    read:    vec![],
                    exclude: Self::EXCLUDE_TYPES.to_vec(),
                }
            }
            fn into_item<'a>(e: Entity, item: Self::QueryItem<'a>) -> Self::Item<'a>
            {
                (e, item)
            }
            unsafe fn build_src_access(arch: *mut Archetype, chunk: *mut Chunk) -> Self::SrcAccess
            {
                let src_chunk = match T::COMPONENT_LOCATION
                {
                    ComponentLocation::Chunk => chunk,
                    ComponentLocation::Archetype =>
                    unsafe { (*arch).shared_chunk_ptr() },
                };
                let c = unsafe { &*src_chunk };
                let (base, col, stride, storage) = c.col_access::<T>();
                SrcAccessMut {
                    base:    base as *mut u8,
                    stride:  stride,
                    storage: storage,
                    col_idx: col,
                    chunk:   src_chunk,
                    _p:      PhantomData,
                }
            }
            unsafe fn is_queriable(accessor: &Self::SrcAccess, row: usize) -> bool
            {
                let r = match T::COMPONENT_LOCATION
                {
                    ComponentLocation::Chunk => row,
                    ComponentLocation::Archetype => 0,
                };
                let ptr = unsafe { &*accessor.chunk };
                ptr.$query_check(accessor.col_idx, r)
            }
            fn fetch<'a>(src: &Self::SrcAccess, idx: usize) -> Self::QueryItem<'a>
            {
                let i = match T::COMPONENT_LOCATION
                {
                    ComponentLocation::Chunk => idx,
                    ComponentLocation::Archetype => 0,
                };
                unsafe {
                    let slot = src.base.add(i * src.stride);
                    match src.storage
                    {
                        ColumnStorage::Inline => &mut *(slot as *mut T),
                        ColumnStorage::Indirect(f) => &mut *(f(slot as *const u8) as *mut T),
                    }
                }
            }
        }
    };
}
macro_rules! query_tuple {
    (($first:ident, $first_idx:tt) $(, ($rest:ident, $rest_idx:tt))* $(,)?) =>
    {
        impl<$first, $($rest,)*> TQueryParam for ($first, $($rest,)*)
        where $first: TQueryParam, $($rest: TQueryParam,)*
        {
            type QueryItem<'a> = ($first::QueryItem<'a>, $($rest::QueryItem<'a>,)*);
            type Item<'a> = (Entity, $first::QueryItem<'a>, $($rest::QueryItem<'a>,)*);
            type SrcAccess = ($first::SrcAccess, $($rest::SrcAccess,)*);
            // luôn trỏ tới &T, &mut T arr chỉ có 1 phần tử
            const ACCESS_TYPES: &'static [TypeId] = &[$first::ACCESS_TYPES[0], $($rest::ACCESS_TYPES[0],)*];
            const EXCLUDE_TYPES: &'static [TypeId] = &[];

            fn access_scope() -> AccessScope
            {
                let mut first_access_scope = $first::access_scope();

                $(first_access_scope.extend($rest::access_scope());)*
                first_access_scope
            }
            fn into_item<'a>(e: Entity, item: Self::QueryItem<'a>) -> Self::Item<'a>
            {
                (e, item.$first_idx, $(item.$rest_idx,)*)
            }
            unsafe fn build_src_access(arch: *mut Archetype, chunk: *mut Chunk) -> Self::SrcAccess
            {
                unsafe {
                    (
                        $first::build_src_access(arch, chunk),
                        $($rest::build_src_access(arch, chunk),)*
                    )
                }
            }
            unsafe fn is_queriable(accessor: &Self::SrcAccess, row: usize) -> bool
            {
                unsafe {
                    $first::is_queriable(&accessor.$first_idx, row)  $(&& $rest::is_queriable(&accessor.$rest_idx, row))*
                }
            }
            fn fetch<'a>(src: &Self::SrcAccess, idx: usize) -> Self::QueryItem<'a>
            {
                (
                    $first::fetch(&src.$first_idx, idx),
                    $($rest::fetch(&src.$rest_idx, idx),)*
                )
            }
        }
    };
}
// `Shared<&T>` / `Shared<&mut T>` — query con trỏ chia sẻ, theo đúng convention `&` / `&mut` như các
// query param khác. Không dùng được `impl_query_param!` thường vì:
//   - `fetch` phải đọc THẲNG struct `ComponentPtr<T>` từ slot, KHÔNG áp deref `Indirect` (macro luôn deref
//     để trả `&T` thật).
//   - `is_queriable` phải lọc thêm theo `ColumnStorage`: chỉ nhận cột `Indirect`, bỏ qua cột `Inline`.
// Lý do cần lọc runtime: `ComponentPtr<T>` forward query-id = `T::QUERY_TYPE_ID`, nên trong query-id
// space archetype `(T,)` (inline) và `(ComponentPtr<T>,)` (con trỏ) KHÔNG phân biệt được bằng type.
// `contains_all(ACCESS_TYPES)` match cả hai; phải dựa vào `ColumnStorage::Indirect` để loại inline lúc chạy.
// Shared chỉ áp dụng cho chunk-component (ComponentPtr luôn nằm trong entity chunk).
macro_rules! impl_shared_query_param {
    ($target:ty) => {
        impl<T: TComponent + 'static> TQueryParam for $target
        {
            type QueryItem<'a> = &'a ComponentPtr<T>;
            type Item<'a> = (Entity, &'a ComponentPtr<T>);
            type SrcAccess = SrcAccess<ComponentPtr<T>>;
            const ACCESS_TYPES: &'static [TypeId] = &[T::QUERY_TYPE_ID];
            const EXCLUDE_TYPES: &'static [TypeId] = &[];

            fn access_scope() -> AccessScope
            {
                AccessScope {
                    read:    vec![T::QUERY_TYPE_ID],
                    write:   vec![],
                    exclude: Self::EXCLUDE_TYPES.to_vec(),
                }
            }
            fn into_item<'a>(e: Entity, item: Self::QueryItem<'a>) -> Self::Item<'a>
            {
                (e, item)
            }

            unsafe fn build_src_access(_arch: *mut Archetype, chunk: *mut Chunk) -> Self::SrcAccess
            {
                let c = unsafe { &*chunk };
                let (base, col, stride, storage) = c.col_access::<ComponentPtr<T>>();
                SrcAccess {
                    base:    base,
                    stride:  stride,
                    storage: storage,
                    col_idx: col,
                    chunk:   chunk,
                    _p:      PhantomData,
                }
            }

            unsafe fn is_queriable(accessor: &Self::SrcAccess, row: usize) -> bool
            {
                // cột `Inline` = entity giữ `T` trực tiếp, ko phải con trỏ chia sẻ → bỏ qua.
                if !matches!(accessor.storage, ColumnStorage::Indirect(_))
                {
                    return false;
                }
                let ptr = unsafe { &*accessor.chunk };
                ptr.has_value_and_active(accessor.col_idx, row)
            }

            fn fetch<'a>(src: &Self::SrcAccess, idx: usize) -> Self::QueryItem<'a>
            {
                // đọc THẲNG struct con trỏ (stride = size_of::<ComponentPtr<T>>()), KHÔNG áp deref `Indirect`.
                unsafe {
                    let slot = src.base.add(idx * src.stride);
                    &*(slot as *const ComponentPtr<T>)
                }
            }
        }
    };
}
