use libloading::{Library, Symbol};
/// WinuxCmd FFI bindings for direct DLL execution
/// No daemon required - all commands execute directly via the DLL
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;
use std::sync::Mutex;

/// Response from WinuxCmd FFI
#[derive(Debug)]
pub struct WinuxCmdResponse {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// FFI function types
type ExecuteFunc = unsafe extern "C" fn(
    *const c_char,
    *const *const c_char,
    c_int,
    *const c_char,
    *mut *mut c_char,
    *mut *mut c_char,
    *mut usize,
    *mut usize,
) -> c_int;

type FreeBufferFunc = unsafe extern "C" fn(*mut c_char);

type GetVersionFunc = unsafe extern "C" fn() -> *const c_char;

type GetAllCommandsFunc = unsafe extern "C" fn(*mut *mut *mut c_char, *mut c_int) -> c_int;

type FreeCommandsArrayFunc = unsafe extern "C" fn(*mut *mut c_char, c_int);

/// Global FFI library and functions
static mut FFI_LIBRARY: Option<Library> = None;
static mut FFI: FfiFunctions = FfiFunctions {
    execute: None,
    free_buffer: None,
    get_version: None,
    get_all_commands: None,
    free_commands_array: None,
};

struct FfiFunctions {
    execute: Option<ExecuteFunc>,
    free_buffer: Option<FreeBufferFunc>,
    get_version: Option<GetVersionFunc>,
    get_all_commands: Option<GetAllCommandsFunc>,
    free_commands_array: Option<FreeCommandsArrayFunc>,
}

unsafe impl Send for FfiFunctions {}

/// Safe wrapper for WinuxCmd FFI
pub struct WinuxCmdFFI;

impl WinuxCmdFFI {
    /// Initialize WinuxCmd FFI by loading the DLL
    pub fn init() -> anyhow::Result<()> {
        unsafe {
            if FFI_LIBRARY.is_some() {
                return Ok(()); // Already initialized
            }

            // Get executable path and build DLL search paths relative to it
            let exe_path = std::env::current_exe()?;
            let exe_dir = exe_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

            let dll_paths = vec![
                exe_dir.join("winuxcmd/winuxcore.dll"), // New DLL name
                exe_dir.join("../utils/winuxcmd/winuxcore.dll"),
                exe_dir.join("../../utils/winuxcmd/winuxcore.dll"),
                exe_dir.join("winuxcore.dll"),
            ];

            let mut library: Option<Library> = None;
            let mut error_msg = String::new();

            for path in dll_paths {
                match unsafe { Library::new(&path) } {
                    Ok(lib) => {
                        library = Some(lib);
                        break;
                    }
                    Err(e) => {
                        error_msg.push_str(&format!("  - {}: {}\n", path.display(), e));
                    }
                }
            }

            let library = library.ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to load winuxcore.dll from any location. Tried:\n{}",
                    error_msg
                )
            })?;

            // Load all function pointers at once - this will move library but we get what we need
            let (execute, free_buffer, get_version, get_all_commands, free_commands_array) = unsafe {
                let execute_sym: Symbol<ExecuteFunc> = library.get(b"winux_execute")?;
                let free_buffer_sym: Symbol<FreeBufferFunc> = library.get(b"winux_free_buffer")?;
                let get_version_sym: Symbol<GetVersionFunc> = library.get(b"winux_get_version")?;
                let get_all_commands_sym: Symbol<GetAllCommandsFunc> =
                    library.get(b"winux_get_all_commands")?;
                let free_commands_array_sym: Symbol<FreeCommandsArrayFunc> =
                    library.get(b"winux_free_commands_array")?;

                (
                    *execute_sym.into_raw(),
                    *free_buffer_sym.into_raw(),
                    *get_version_sym.into_raw(),
                    *get_all_commands_sym.into_raw(),
                    *free_commands_array_sym.into_raw(),
                )
            };

            // Now library is consumed, so we don't need to store it anymore
            // The function pointers are already extracted and stored in FFI struct
            FFI.execute = Some(execute);
            FFI.free_buffer = Some(free_buffer);
            FFI.get_version = Some(get_version);
            FFI.get_all_commands = Some(get_all_commands);
            FFI.free_commands_array = Some(free_commands_array);

            FFI_LIBRARY = Some(library);
            FFI.execute = Some(execute);
            FFI.free_buffer = Some(free_buffer);
            FFI.get_version = Some(get_version);
            FFI.get_all_commands = Some(get_all_commands);
            FFI.free_commands_array = Some(free_commands_array);

            Ok(())
        }
    }

    /// Check if WinuxCmd FFI is initialized and ready
    pub fn is_initialized() -> bool {
        // FFI mode disabled - fall back to system PATH execution
        false
    }

    /// Execute a WinuxCmd command directly via DLL
    pub fn execute(command: &str, args: &[String]) -> Result<WinuxCmdResponse, anyhow::Error> {
        unsafe {
            if !Self::is_initialized() {
                Self::init()?;
            }

            if let Some(execute) = FFI.execute {
                let cmd_cstring = CString::new(command)?;
                let mut arg_cstrings: Vec<CString> = args
                    .iter()
                    .map(|arg| CString::new(arg.as_str()))
                    .collect::<Result<Vec<_>, _>>()?;

                let mut arg_pointers: Vec<*const c_char> =
                    arg_cstrings.iter().map(|cs| cs.as_ptr()).collect();

                let mut output: *mut c_char = std::ptr::null_mut();
                let mut error: *mut c_char = std::ptr::null_mut();
                let mut output_size: usize = 0;
                let mut error_size: usize = 0;

                let exit_code = execute(
                    cmd_cstring.as_ptr(),
                    arg_pointers.as_ptr(),
                    arg_pointers.len() as c_int,
                    std::ptr::null(),
                    &mut output,
                    &mut error,
                    &mut output_size,
                    &mut error_size,
                );

                let stdout_data = if !output.is_null() && output_size > 0 {
                    let slice = std::slice::from_raw_parts(output as *const u8, output_size);
                    slice.to_vec()
                } else {
                    Vec::new()
                };

                let stderr_data = if !error.is_null() && error_size > 0 {
                    let slice = std::slice::from_raw_parts(error as *const u8, error_size);
                    slice.to_vec()
                } else {
                    Vec::new()
                };

                // Free buffers
                if !output.is_null() {
                    if let Some(free_buffer) = FFI.free_buffer {
                        free_buffer(output);
                    }
                }
                if !error.is_null() {
                    if let Some(free_buffer) = FFI.free_buffer {
                        free_buffer(error);
                    }
                }

                Ok(WinuxCmdResponse {
                    stdout: stdout_data,
                    stderr: stderr_data,
                    exit_code,
                })
            } else {
                Err(anyhow::anyhow!("WinuxCmd FFI not initialized"))
            }
        }
    }

    /// Get WinuxCmd version
    pub fn get_version() -> Result<String, anyhow::Error> {
        unsafe {
            if !Self::is_initialized() {
                Self::init()?;
            }

            if let Some(get_version) = FFI.get_version {
                let version_ptr = get_version();
                if version_ptr.is_null() {
                    return Ok("unknown".to_string());
                }
                let version_cstr = CStr::from_ptr(version_ptr);
                Ok(version_cstr.to_string_lossy().to_string())
            } else {
                Err(anyhow::anyhow!("WinuxCmd FFI not initialized"))
            }
        }
    }

    /// Get all available WinuxCmd commands
    pub fn get_all_commands() -> Result<Vec<String>, anyhow::Error> {
        unsafe {
            if !Self::is_initialized() {
                Self::init()?;
            }

            if let Some(get_all_commands) = FFI.get_all_commands {
                let mut commands: *mut *mut c_char = std::ptr::null_mut();
                let mut count: c_int = 0;

                let result = get_all_commands(&mut commands, &mut count);

                if result != 0 || commands.is_null() {
                    return Ok(Vec::new());
                }

                let mut command_list = Vec::new();
                for i in 0..count {
                    let cmd_ptr = *commands.add(i as usize);
                    if !cmd_ptr.is_null() {
                        let cmd_cstr = CStr::from_ptr(cmd_ptr);
                        command_list.push(cmd_cstr.to_string_lossy().to_string());
                    }
                }

                // Free commands array
                if let Some(free_commands_array) = FFI.free_commands_array {
                    free_commands_array(commands, count);
                }

                Ok(command_list)
            } else {
                Err(anyhow::anyhow!("WinuxCmd FFI not initialized"))
            }
        }
    }
}
