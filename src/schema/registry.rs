use crate::error::Error;

use super::types::TableDef;

#[doc(hidden)]
#[repr(C)]
pub struct RegistryEntry {
    build_table: fn() -> Option<TableDef>,
}

impl RegistryEntry {
    #[doc(hidden)]
    pub const fn new(build_table: fn() -> Option<TableDef>) -> Self {
        Self { build_table }
    }

    fn build(&self) -> Option<TableDef> {
        (self.build_table)()
    }
}

fn sentinel() -> Option<TableDef> {
    None
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#[used]
#[unsafe(link_section = "seekwel_schema_registry")]
static SEEKWEL_SCHEMA_REGISTRY_SENTINEL: RegistryEntry = RegistryEntry::new(sentinel);

pub(crate) fn registered_tables() -> Result<Vec<TableDef>, Error> {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        let entries = unsafe { registry_entries() };
        return Ok(entries.iter().filter_map(RegistryEntry::build).collect());
    }

    #[allow(unreachable_code)]
    Err(Error::InvalidSchema(
        "automatic model registry is only supported on ELF-like targets in v1".into(),
    ))
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
unsafe fn registry_entries() -> &'static [RegistryEntry] {
    #[allow(improper_ctypes)]
    unsafe extern "C" {
        static __start_seekwel_schema_registry: RegistryEntry;
        static __stop_seekwel_schema_registry: RegistryEntry;
    }

    let start = std::ptr::addr_of!(__start_seekwel_schema_registry);
    let stop = std::ptr::addr_of!(__stop_seekwel_schema_registry);
    let len = unsafe { stop.offset_from(start) as usize };
    unsafe { std::slice::from_raw_parts(start, len) }
}

