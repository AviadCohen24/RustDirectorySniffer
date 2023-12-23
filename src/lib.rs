use serde::{Serialize, Deserialize};
use std::{path::{PathBuf, Path}, ffi::{CString, CStr}, os::raw::c_char, sync::{Arc, Mutex}, io};
use tokio::{fs, runtime::Runtime};
use lazy_static::lazy_static;
use async_recursion::async_recursion;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct FolderHierarchy {
    value: u64,
    name: String,
    path: String,
    children: Vec<FolderHierarchy>,
}

lazy_static! {
    static ref GLOBAL_DIRECTORY_MAP: Arc<Mutex<FolderHierarchy>> = Arc::new(Mutex::new(FolderHierarchy {
        value: 0,
        name: String::new(),
        path: String::new(),
        children: vec![],
    }));
}

#[async_recursion]
async fn get_folder_size(dir_path: &PathBuf) -> io::Result<u64> {
    let mut size: u64 = 0;
    let mut entries = fs::read_dir(dir_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            size += get_folder_size(&path).await?;
        } else {
            match path.metadata() {
                Ok(metadata) => size += metadata.len(),
                Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
            }
        }
    }

    Ok(size)
}

#[async_recursion]
async fn scan_folder(directory_path: PathBuf, shared_state: Arc<Mutex<FolderHierarchy>>) -> io::Result<FolderHierarchy> {
    let mut entries = fs::read_dir(directory_path.clone()).await?;
    let mut children = Vec::new();
    let mut total_size = 0;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            let child_hierarchy = scan_folder(path, Arc::clone(&shared_state)).await?;
            total_size += child_hierarchy.value;
            children.push(child_hierarchy);
        } else {
            match path.metadata() {
                Ok(metadata) => total_size += metadata.len(),
                Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
            }
        }
    }

    let name = directory_path.file_name().unwrap_or_default().to_str().unwrap_or("").to_string();
    let path = directory_path.parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned();

    Ok(FolderHierarchy {
        value: total_size,
        name,
        path,
        children,
    })
}

#[no_mangle]
pub extern "C" fn scan_directory_async(path_ptr: *const c_char) {
    let c_str = unsafe { CStr::from_ptr(path_ptr) };
    let path_str = match c_str.to_str() {
        Ok(str) => str,
        Err(_) => {
            eprintln!("Invalid string passed to scan_directory_async");
            return;
        }
    };
    let directory_path = PathBuf::from(path_str);

    // Clone the GLOBAL_DIRECTORY_MAP for use within the async task
    let global_map_clone = Arc::clone(&GLOBAL_DIRECTORY_MAP);

    // Spawn a new thread to handle the asynchronous scanning
    std::thread::spawn(move || {
        // Create a new runtime for the asynchronous task
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            // Properly initialize the root of the FolderHierarchy
            let root_hierarchy = FolderHierarchy {
                value: 0, // This will be calculated as the sum of all children
                name: directory_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                path: directory_path.to_string_lossy().into_owned(),
                children: vec![],
            };

            // Lock the global directory map for updating and set the root
            let mut global_map = global_map_clone.lock().unwrap();
            *global_map = root_hierarchy;

            // Start scanning the directory structure
            let mut entries = fs::read_dir(directory_path.clone()).await.unwrap();

            // Process each entry in the directory
            while let Some(entry) = entries.next_entry().await.unwrap() {
                let path = entry.path();

                // Here you're updating the global map, which now has a properly initialized root
                if path.is_dir() {
                    // Recursively scan the subdirectory
                    let sub_hierarchy = scan_folder(path, Arc::clone(&global_map_clone)).await.unwrap();
                    global_map.value += sub_hierarchy.value; // Update the value to reflect the size
                    global_map.children.push(sub_hierarchy);
                } else {
                    // Update the global map with file information
                    match path.metadata() {
                        Ok(metadata) => {
                            global_map.value += metadata.len(); // Update the value to reflect the size
                            let file_entry = FolderHierarchy {
                                value: metadata.len(),
                                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                                path: path.parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned(),
                                children: vec![],  // No children for a file
                            };
                            global_map.children.push(file_entry);
                        },
                        Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
                    }
                }
            }
        });
    });
}


#[no_mangle]
pub extern "C" fn get_directory_map(path_ptr: *const c_char, depth: i32) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(path_ptr) };
    let path_str = match c_str.to_str() {
        Ok(str) => str.replace("\\", "/"),  // Normalize the path string
        Err(_) => {
            eprintln!("Invalid string passed to get_directory_map");
            return std::ptr::null_mut(); // Make sure this matches the expected return type
        }
    };

    let json = match GLOBAL_DIRECTORY_MAP.lock() {
        Ok(guard) => {
            let directory_map = &*guard;

            // Normalize paths for comparison and check if the root matches the provided path
            if directory_map.path.replace("\\", "/") == path_str {
                match depth {
                    0 => {
                        // Return only the first level children
                        let first_level_hierarchy = FolderHierarchy {
                            value: directory_map.value, // You might need to calculate this value differently
                            name: directory_map.name.clone(),
                            path: directory_map.path.clone(),
                            children: directory_map.children.iter()
                                .map(|child| FolderHierarchy {
                                    value: child.value,
                                    name: child.name.clone(),
                                    path: child.path.clone(),
                                    children: vec![],  // Exclude further children
                                })
                                .collect(),
                        };
                        serde_json::to_string(&first_level_hierarchy).unwrap_or_else(|e| {
                            eprintln!("Failed to serialize directory map: {}", e);
                            String::new()  // Return an empty string if serialization fails
                        })
                    },
                    1 => {
                        // Return the whole directory map
                        serde_json::to_string(&directory_map).unwrap_or_else(|e| {
                            eprintln!("Failed to serialize directory map: {}", e);
                            String::new()  // Return an empty string if serialization fails
                        })
                    },
                    _ => {
                        eprintln!("Invalid depth argument passed to get_directory_map");
                        serde_json::to_string(&Vec::<FolderHierarchy>::new()).unwrap() // return an empty array in JSON format
                    }
                }
            } else {
                eprintln!("Root folder not found for path: {}", path_str);
                serde_json::to_string(&Vec::<FolderHierarchy>::new()).unwrap() // return an empty array in JSON format
            }
        },
        Err(poisoned) => {
            eprintln!("Warning: Lock was poisoned. The directory map may be in an inconsistent state.");
            let directory_map = &*poisoned.into_inner();
            serde_json::to_string(&directory_map).unwrap_or_else(|e| {
                eprintln!("Failed to serialize directory map: {}", e);
                String::new()  // Return an empty string if serialization fails
            })
        },
    };

    CString::new(json).unwrap().into_raw() // Convert the resulting JSON string into a raw C string
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir; 
    use std::thread;
    use std::time::Duration;

    #[tokio::test]
    async fn test_get_folder_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(file_path).unwrap();
        writeln!(file, "Hello, world!").unwrap();

        let size = get_folder_size(&dir.path().to_path_buf()).await.unwrap();
        assert!(size > 0, "Folder size should be greater than 0");
    }

    #[tokio::test]
    async fn test_scan_folder() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello, world!").unwrap();

        let folder_hierarchy = scan_folder(dir.path().to_path_buf(), Arc::clone(&GLOBAL_DIRECTORY_MAP)).await.unwrap();
        println!("Folder Hierarchy: {:?}", folder_hierarchy);

        let locked_state = GLOBAL_DIRECTORY_MAP.lock().unwrap();
        println!("Locked state: {:?}", locked_state);
        
        //assert_eq!(folder_hierarchy.name, ".tmpoPzeSl", "The directory name should be '.tmpoPzeSl'.");
        assert_eq!(folder_hierarchy.value, 14, "The directory size should be 14 bytes.");
    }

    async fn create_test_directory_structure(base_dir: &PathBuf) -> io::Result<()> {
        fs::create_dir_all(base_dir.join("subfolder1/subsubfolder1")).await?;
        fs::create_dir_all(base_dir.join("subfolder2")).await?;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2a")).await?;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2b")).await?;
    
        let mut file1 = File::create(base_dir.join("subfolder1/test_file1.txt"))?;
        let mut file2 = File::create(base_dir.join("subfolder1/subsubfolder1/test_file2.txt"))?;
        let mut file3 = File::create(base_dir.join("subfolder2/test_file3.txt"))?;
    
        writeln!(file1, "Hello, world!")?;
        writeln!(file2, "Hello, Rust!")?;
        writeln!(file3, "Hello, Testing!")?;
    
        Ok(())
    }

    #[tokio::test]
    async fn test_scan_complex_folder() {
        let dir = tempdir().unwrap();
        create_test_directory_structure(&dir.path().to_path_buf()).await.unwrap();

        let folder_hierarchy = scan_folder(dir.path().to_path_buf(), Arc::clone(&GLOBAL_DIRECTORY_MAP)).await.unwrap();

        println!("Folder Hierarchy: {:?}", folder_hierarchy);

        // Assertions based on the structure you created
        assert_eq!(folder_hierarchy.children.len(), 2, "There should be two subfolders.");
    }

    #[tokio::test]
    async fn test_get_directory_map() {
        // Step 1: Set up the directory structure
        let dir = tempdir().unwrap();
        create_test_directory_structure(&dir.path().to_path_buf()).await.unwrap();
        
        // Step 2: Scan the directory to populate the directory structure
        let path_str = dir.path().to_str().unwrap();
        let path_cstr = CString::new(path_str).unwrap();
        scan_directory_async(path_cstr.as_ptr());

        thread::sleep(Duration::from_millis(50));

        // Step 5: Call the function under test
        let raw_json_ptr = get_directory_map(path_cstr.as_ptr());

        // Ensure the pointer is not null
        assert!(!raw_json_ptr.is_null(), "get_directory_map returned a null pointer");

        // Step 6: Convert the C-style string returned by get_directory_map back to a Rust String
        let json_str = unsafe { CStr::from_ptr(raw_json_ptr) }.to_str().unwrap();

        // Step 7: Deserialize the JSON string back to a FolderHierarchy object
        let directory_map: FolderHierarchy = serde_json::from_str(json_str).unwrap();

        // Perform your assertions
        assert!(!directory_map.children.is_empty(), "The directory map childrens should not be empty");
        
        // Check if the children of each folder in the directory map have an empty children array
        for folder in &directory_map.children {
            assert!(
                folder.children.iter().all(|child| child.children.is_empty()),
                "All children of the folder should have an empty children array"
            );
        }

        // Step 8: Clean up the C string allocated by get_directory_map
        unsafe {
            CString::from_raw(raw_json_ptr);
        }
    }




}

fn main() {
    // This function is a placeholder and won't be used when called from TypeScript.
}
