//! The CoreFoundation, CoreGraphics and IOKit calls the macOS link needs, plus small owning wrappers.
//!
//! Type mapping (LP64): CF*Ref → `*const c_void`, CFIndex → isize, CFTypeID → usize, io_object_t and friends
//! (mach_port_t) → u32, kern_return_t / IOReturn → i32, boolean_t → i32, Boolean → u8.
//! The private calls (IOAVService*, CoreDisplay_DisplayCreateInfoDictionary) are resolved with dlsym, so a macOS
//! without them makes `connect` report NotFound instead of the app failing to launch.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

pub(crate) type CFTypeRef = *const c_void;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT64_TYPE: isize = 4;
pub(crate) const IO_MAIN_PORT_DEFAULT: u32 = 0;
pub(crate) const IO_REGISTRY_ITERATE_RECURSIVELY: u32 = 1;
/// kIOServicePlane.
pub(crate) const SERVICE_PLANE: &CStr = c"IOService";

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(alloc: CFTypeRef, c_str: *const c_char, encoding: u32) -> CFTypeRef;
    fn CFStringGetCString(string: CFTypeRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value: *mut c_void) -> u8;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub(crate) fn CGGetOnlineDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    pub(crate) fn CGDisplayVendorNumber(display: u32) -> u32;
    pub(crate) fn CGDisplayModelNumber(display: u32) -> u32;
    pub(crate) fn CGDisplayIsBuiltin(display: u32) -> i32;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IORegistryGetRootEntry(main_port: u32) -> u32;
    fn IORegistryEntryCreateIterator(entry: u32, plane: *const c_char, options: u32, iterator: *mut u32) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOObjectConformsTo(object: u32, class_name: *const c_char) -> i32;
    fn IORegistryEntryGetName(entry: u32, name: *mut c_char) -> i32;
    fn IORegistryEntryGetRegistryEntryID(entry: u32, id: *mut u64) -> i32;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> i32;
    fn IORegistryEntryCopyFromPath(main_port: u32, path: CFTypeRef) -> u32;
    fn IORegistryEntryCreateCFProperty(entry: u32, key: CFTypeRef, alloc: CFTypeRef, options: u32) -> CFTypeRef;
    fn IORegistryEntrySearchCFProperty(
        entry: u32,
        plane: *const c_char,
        key: CFTypeRef,
        alloc: CFTypeRef,
        options: u32,
    ) -> CFTypeRef;
}

// ----- CoreFoundation -----

/// An owned CF object (Create/Copy rule), released on drop. May be null.
pub(crate) struct Cf(pub(crate) CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: we own one reference to this non-null CF object.
            unsafe { CFRelease(self.0) };
        }
    }
}

impl Cf {
    pub(crate) fn string(value: &str) -> Cf {
        let c = CString::new(value).unwrap_or_default();
        // SAFETY: `c` is a valid NUL-terminated string for the call; the result follows the Create rule.
        Cf(unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), CF_STRING_ENCODING_UTF8) })
    }
}

/// Copies a CFString (borrowed) into a Rust string. None if null or not a string.
pub(crate) fn cf_to_string(cf: CFTypeRef) -> Option<String> {
    // SAFETY: type-checked CF calls on a non-null CF object; the buffer is large enough for `size` bytes.
    unsafe {
        if cf.is_null() || CFGetTypeID(cf) != CFStringGetTypeID() {
            return None;
        }
        let mut buffer = [0 as c_char; 1024];
        (CFStringGetCString(cf, buffer.as_mut_ptr(), buffer.len() as isize, CF_STRING_ENCODING_UTF8) != 0)
            .then(|| CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned())
    }
}

/// Reads a CFNumber (borrowed) as i64. None if null or not a number.
pub(crate) fn cf_to_i64(cf: CFTypeRef) -> Option<i64> {
    // SAFETY: type-checked CF calls on a non-null CF object; `value` is a valid i64 out-pointer.
    unsafe {
        if cf.is_null() || CFGetTypeID(cf) != CFNumberGetTypeID() {
            return None;
        }
        let mut value: i64 = 0;
        (CFNumberGetValue(cf, CF_NUMBER_SINT64_TYPE, (&mut value as *mut i64).cast()) != 0).then_some(value)
    }
}

/// Looks up a string key in a CFDictionary. The result is borrowed (Get rule); null if absent or not a dictionary.
pub(crate) fn cf_dict_get(dict: CFTypeRef, key: &str) -> CFTypeRef {
    // SAFETY: type-checked CF call on a non-null dictionary with a valid key that outlives the call.
    unsafe {
        if dict.is_null() || CFGetTypeID(dict) != CFDictionaryGetTypeID() {
            return std::ptr::null();
        }
        let key = Cf::string(key);
        CFDictionaryGetValue(dict, key.0)
    }
}

// ----- IOKit -----

/// An owned io_object_t, released on drop. May be 0.
pub(crate) struct IoObject(pub(crate) u32);

impl Drop for IoObject {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: we own this IOKit object reference.
            unsafe { IOObjectRelease(self.0) };
        }
    }
}

impl IoObject {
    pub(crate) fn root() -> IoObject {
        // SAFETY: plain IOKit call; the result is owned.
        IoObject(unsafe { IORegistryGetRootEntry(IO_MAIN_PORT_DEFAULT) })
    }

    pub(crate) fn from_path(path: &str) -> IoObject {
        let path = Cf::string(path);
        // SAFETY: valid CFString; the result is owned (Copy rule).
        IoObject(unsafe { IORegistryEntryCopyFromPath(IO_MAIN_PORT_DEFAULT, path.0) })
    }

    /// A recursive iterator over the IOService plane below this entry.
    pub(crate) fn iterate_recursively(&self) -> Option<IoObject> {
        let mut iterator = 0;
        // SAFETY: valid plane name and out-pointer; the iterator is owned.
        let kr = unsafe {
            IORegistryEntryCreateIterator(self.0, SERVICE_PLANE.as_ptr(), IO_REGISTRY_ITERATE_RECURSIVELY, &mut iterator)
        };
        (kr == 0).then_some(IoObject(iterator))
    }

    /// The next entry of an iterator, or None at the end.
    pub(crate) fn next(&self) -> Option<IoObject> {
        // SAFETY: plain IOKit call on an iterator; the result is owned.
        let entry = unsafe { IOIteratorNext(self.0) };
        (entry != 0).then_some(IoObject(entry))
    }

    pub(crate) fn name(&self) -> Option<String> {
        let mut name = [0 as c_char; 128]; // io_name_t
        // SAFETY: `name` is the io_name_t buffer the call fills with a NUL-terminated string.
        unsafe { (IORegistryEntryGetName(self.0, name.as_mut_ptr()) == 0).then(|| CStr::from_ptr(name.as_ptr())) }
            .map(|s| s.to_string_lossy().into_owned())
    }

    pub(crate) fn registry_id(&self) -> Option<u64> {
        let mut id = 0;
        // SAFETY: valid out-pointer.
        (unsafe { IORegistryEntryGetRegistryEntryID(self.0, &mut id) } == 0).then_some(id)
    }

    pub(crate) fn conforms_to(&self, class: &CStr) -> bool {
        // SAFETY: NUL-terminated class name.
        unsafe { IOObjectConformsTo(self.0, class.as_ptr()) != 0 }
    }

    pub(crate) fn parent(&self) -> Option<IoObject> {
        let mut parent = 0;
        // SAFETY: valid plane name and out-pointer; the parent is owned.
        let kr = unsafe { IORegistryEntryGetParentEntry(self.0, SERVICE_PLANE.as_ptr(), &mut parent) };
        (kr == 0 && parent != 0).then_some(IoObject(parent))
    }

    /// A property of this entry only (owned).
    pub(crate) fn property(&self, key: &str) -> Cf {
        let key = Cf::string(key);
        // SAFETY: valid CFString key; the result follows the Create rule.
        Cf(unsafe { IORegistryEntryCreateCFProperty(self.0, key.0, std::ptr::null(), 0) })
    }

    /// Searches this entry, then its IOService-plane children, for a property (owned).
    pub(crate) fn search_property(&self, key: &str) -> Cf {
        let key = Cf::string(key);
        // SAFETY: valid plane name and CFString key; the result follows the Create rule.
        Cf(unsafe {
            IORegistryEntrySearchCFProperty(
                self.0,
                SERVICE_PLANE.as_ptr(),
                key.0,
                std::ptr::null(),
                IO_REGISTRY_ITERATE_RECURSIVELY,
            )
        })
    }
}

// ----- private calls, resolved at run time -----

type CreateInfoDictionary = unsafe extern "C" fn(display: u32) -> CFTypeRef;
type AvCreate = unsafe extern "C" fn(alloc: CFTypeRef, service: u32) -> CFTypeRef;
type AvReadI2c = unsafe extern "C" fn(service: CFTypeRef, chip: u32, offset: u32, buf: *mut c_void, len: u32) -> i32;
type AvWriteI2c = unsafe extern "C" fn(service: CFTypeRef, chip: u32, data_addr: u32, buf: *mut c_void, len: u32) -> i32;

/// The private IOAVService entry points in IOKit (as declared by m1ddc and MonitorControl, both MIT).
#[derive(Clone, Copy)]
pub(crate) struct IoAv {
    pub(crate) create: AvCreate,
    pub(crate) read_i2c: AvReadI2c,
    pub(crate) write_i2c: AvWriteI2c,
}

/// The IOAVService calls, or None when this macOS doesn't export them (Intel Macs before 11, future removals).
pub(crate) fn io_av() -> Option<IoAv> {
    static FNS: OnceLock<Option<IoAv>> = OnceLock::new();
    *FNS.get_or_init(|| {
        let lib = dlopen(c"/System/Library/Frameworks/IOKit.framework/IOKit")?;
        // SAFETY: the symbols have these signatures (m1ddc / MonitorControl declarations).
        unsafe {
            Some(IoAv {
                create: std::mem::transmute::<*mut c_void, AvCreate>(dlsym(lib, c"IOAVServiceCreateWithService")?),
                read_i2c: std::mem::transmute::<*mut c_void, AvReadI2c>(dlsym(lib, c"IOAVServiceReadI2C")?),
                write_i2c: std::mem::transmute::<*mut c_void, AvWriteI2c>(dlsym(lib, c"IOAVServiceWriteI2C")?),
            })
        }
    })
}

/// CoreDisplay's info dictionary for a display (owned; null for Sidecar/AirPlay or when CoreDisplay is unavailable).
/// m1ddc links it from /System/Library/Frameworks; PrivateFrameworks is tried as a fallback.
pub(crate) fn display_info_dictionary(display: u32) -> Cf {
    static FN: OnceLock<Option<CreateInfoDictionary>> = OnceLock::new();
    let create = FN.get_or_init(|| {
        [
            c"/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay",
            c"/System/Library/PrivateFrameworks/CoreDisplay.framework/CoreDisplay",
        ]
        .into_iter()
        .find_map(|path| dlsym(dlopen(path)?, c"CoreDisplay_DisplayCreateInfoDictionary"))
        // SAFETY: CFDictionaryRef CoreDisplay_DisplayCreateInfoDictionary(CGDirectDisplayID).
        .map(|f| unsafe { std::mem::transmute::<*mut c_void, CreateInfoDictionary>(f) })
    });
    // SAFETY: the resolved function takes a display id and returns an owned dictionary or null.
    Cf(create.map_or(std::ptr::null(), |f| unsafe { f(display) }))
}

fn dlopen(path: &CStr) -> Option<*mut c_void> {
    // SAFETY: NUL-terminated path; the handle is kept for the life of the process.
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
    (!handle.is_null()).then_some(handle)
}

fn dlsym(handle: *mut c_void, name: &CStr) -> Option<*mut c_void> {
    // SAFETY: valid handle from dlopen and a NUL-terminated symbol name.
    let symbol = unsafe { libc::dlsym(handle, name.as_ptr()) };
    (!symbol.is_null()).then_some(symbol)
}
