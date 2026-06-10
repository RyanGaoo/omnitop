//! GPU monitoring.
//!
//! On macOS this reads the IORegistry's accelerator entries via IOKit and pulls the
//! `PerformanceStatistics` dictionary (the same source Activity Monitor uses). Crucially
//! this works **without root**, unlike `powermetrics`.
//!
//! NVIDIA (NVML) and other backends are planned; on unsupported platforms `sample()`
//! simply returns an empty list.

#[derive(Clone)]
pub struct GpuStats {
    pub name: String,
    pub utilization: Option<f32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
}

/// Sample all detected GPUs. Returns an empty vec if none are found or the platform
/// is unsupported.
pub fn sample() -> Vec<GpuStats> {
    #[cfg(target_os = "macos")]
    {
        macos::sample()
    }
    #[cfg(not(target_os = "macos"))]
    {
        nvidia::sample()
    }
}

#[cfg(not(target_os = "macos"))]
mod nvidia {
    use nvml_wrapper::Nvml;

    use super::GpuStats;

    pub fn sample() -> Vec<GpuStats> {
        // NVML is loaded at runtime; on a machine without an NVIDIA driver `init` fails
        // and we return nothing, so the GPU panel simply doesn't appear.
        let Ok(nvml) = Nvml::init() else {
            return Vec::new();
        };
        let Ok(count) = nvml.device_count() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for index in 0..count {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };
            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
            let utilization = device.utilization_rates().ok().map(|u| u.gpu as f32);
            let (mem_used, mem_total) = match device.memory_info() {
                Ok(mem) => (Some(mem.used), Some(mem.total)),
                Err(_) => (None, None),
            };
            out.push(GpuStats {
                name,
                utilization,
                mem_used,
                mem_total,
            });
        }
        out
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::ptr;

    use core_foundation_sys::base::{kCFAllocatorDefault, CFRelease, CFTypeRef};
    use core_foundation_sys::dictionary::{
        CFDictionaryGetValueIfPresent, CFDictionaryRef, CFMutableDictionaryRef,
    };
    use core_foundation_sys::number::{kCFNumberFloat64Type, CFNumberGetValue, CFNumberRef};
    use core_foundation_sys::string::{
        kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringRef,
    };

    use super::GpuStats;

    // mach_port_t / io_object_t / io_iterator_t are all u32 on Apple platforms.
    type IoObject = u32;
    const KERN_SUCCESS: i32 = 0;
    const KIO_MAIN_PORT_DEFAULT: u32 = 0;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
        fn IOServiceGetMatchingServices(
            main_port: u32,
            matching: CFDictionaryRef,
            existing: *mut IoObject,
        ) -> i32;
        fn IOIteratorNext(iterator: IoObject) -> IoObject;
        fn IORegistryEntryCreateCFProperties(
            entry: IoObject,
            properties: *mut CFMutableDictionaryRef,
            allocator: *const c_void,
            options: u32,
        ) -> i32;
        fn IORegistryEntryGetName(entry: IoObject, name: *mut c_char) -> i32;
        fn IOObjectRelease(object: IoObject) -> i32;
    }

    pub fn sample() -> Vec<GpuStats> {
        // AGXAccelerator is the Apple Silicon GPU class; IOAccelerator is the parent
        // (also covers older/Intel GPUs). Try each until one yields entries.
        for class in ["IOAccelerator", "AGXAccelerator"] {
            let gpus = unsafe { sample_class(class) };
            if !gpus.is_empty() {
                return gpus;
            }
        }
        Vec::new()
    }

    unsafe fn sample_class(class: &str) -> Vec<GpuStats> {
        let mut out = Vec::new();
        let Ok(cls) = CString::new(class) else {
            return out;
        };

        let matching = IOServiceMatching(cls.as_ptr());
        if matching.is_null() {
            return out;
        }

        // IOServiceGetMatchingServices consumes a reference on `matching`; do not release it.
        let mut iter: IoObject = 0;
        if IOServiceGetMatchingServices(
            KIO_MAIN_PORT_DEFAULT,
            matching as CFDictionaryRef,
            &mut iter,
        ) != KERN_SUCCESS
        {
            return out;
        }

        loop {
            let entry = IOIteratorNext(iter);
            if entry == 0 {
                break;
            }
            if let Some(gpu) = read_entry(entry) {
                out.push(gpu);
            }
            IOObjectRelease(entry);
        }
        IOObjectRelease(iter);
        out
    }

    unsafe fn read_entry(entry: IoObject) -> Option<GpuStats> {
        let mut props: CFMutableDictionaryRef = ptr::null_mut();
        if IORegistryEntryCreateCFProperties(entry, &mut props, kCFAllocatorDefault, 0)
            != KERN_SUCCESS
            || props.is_null()
        {
            return None;
        }
        let props_ref = props as CFDictionaryRef;

        let mut name_buf = [0 as c_char; 128];
        let name = if IORegistryEntryGetName(entry, name_buf.as_mut_ptr()) == KERN_SUCCESS {
            CStr::from_ptr(name_buf.as_ptr())
                .to_string_lossy()
                .into_owned()
        } else {
            "GPU".to_string()
        };

        // PerformanceStatistics is a nested dictionary; without it this isn't a usable GPU.
        let perf_key = cfstr("PerformanceStatistics");
        let mut perf_val: CFTypeRef = ptr::null();
        let has_perf =
            CFDictionaryGetValueIfPresent(props_ref, perf_key as *const c_void, &mut perf_val) != 0;
        CFRelease(perf_key as CFTypeRef);

        let result = if has_perf && !perf_val.is_null() {
            // perf_val is borrowed from props; do not release it separately.
            let perf = perf_val as CFDictionaryRef;
            let utilization = dict_number(perf, "Device Utilization %").map(|v| v as f32);
            let mem_used = dict_number(perf, "In use system memory").map(|v| v as u64);
            let mem_total = dict_number(perf, "Alloc system memory").map(|v| v as u64);
            Some(GpuStats {
                name,
                utilization,
                mem_used,
                mem_total,
            })
        } else {
            None
        };

        CFRelease(props_ref as CFTypeRef);
        result
    }

    /// Look up a numeric value in a CFDictionary by string key.
    unsafe fn dict_number(dict: CFDictionaryRef, key: &str) -> Option<f64> {
        let cf_key = cfstr(key);
        let mut value: CFTypeRef = ptr::null();
        let present = CFDictionaryGetValueIfPresent(dict, cf_key as *const c_void, &mut value);
        CFRelease(cf_key as CFTypeRef);
        if present == 0 || value.is_null() {
            return None;
        }

        let mut out: f64 = 0.0;
        let ok = CFNumberGetValue(
            value as CFNumberRef,
            kCFNumberFloat64Type,
            &mut out as *mut f64 as *mut c_void,
        );
        if ok {
            Some(out)
        } else {
            None
        }
    }

    /// Create an owned CFString from a Rust &str. Caller must CFRelease it.
    unsafe fn cfstr(s: &str) -> CFStringRef {
        let c = CString::new(s).unwrap_or_default();
        CFStringCreateWithCString(kCFAllocatorDefault, c.as_ptr(), kCFStringEncodingUTF8)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Prints whatever GPUs the IORegistry exposes on this machine. Ignored by default
    /// since it depends on hardware. Run with:
    /// `cargo test -- --ignored --nocapture live_gpu`
    #[test]
    #[ignore = "hardware dependent; run manually on a real machine"]
    fn live_gpu_sample() {
        let gpus = sample();
        println!("found {} GPU(s):", gpus.len());
        for g in &gpus {
            println!(
                "  {:24}  util={:?}  mem_used={:?}  mem_total={:?}",
                g.name, g.utilization, g.mem_used, g.mem_total
            );
        }
        assert!(!gpus.is_empty(), "expected at least one GPU on macOS");
    }
}
