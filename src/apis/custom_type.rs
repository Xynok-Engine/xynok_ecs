pub type FnComponentDropItSelf = fn(*mut u8);

/// `std::any::type_name` is not a const fn, so a descriptor cannot store the name itself.
/// Storing the function and calling it on demand works in a const, exactly like `fn_drop` does,
/// and costs nothing until something actually needs to print the name.
pub type FnComponentName = fn() -> &'static str;

//pub type FnArchtypeRemoveEntity = fn(&ChunkLayout, &ComponentSpecs, &mut Chunk, usize) -> Result<Option<SwappedRow>, XynokEcsError>;
//pub type FnCloneComponent = fn(*const u8, *mut u8);
