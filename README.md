# Rust Folder Hierarchy DLL

This repository contains a Rust-compiled Dynamic Link Library (DLL) named `directory_scanner.dll`, designed for efficient folder hierarchy scanning and manipulation. It provides fast and reliable access to file system operations, particularly suited for applications requiring extensive directory traversal and organization.

## Features

- **Scan Directory**: Recursively scans directories and constructs a hierarchical structure of folders and files.
- **Get Directory Map**: Retrieves the hierarchical structure of a specified directory. Users can choose to retrieve only the first level or the entire directory map.
- **Asynchronous Scanning**: Leverages Rust's powerful async/await features for non-blocking directory scanning.
- **FFI Support**: Includes functionality to be called from other languages via FFI (Foreign Function Interface), particularly useful for integrating with C or TypeScript projects.
- **Thread Safety**: Utilizes `Arc<Mutex<>>` to safely share state between threads.
- **Incremental Updates**: Supports the ability to stop the scanning process mid-way.

## Functions

### `create_directory_scanner`

Allocates and initializes a new `DirectoryScanner`.

- **Returns**: A pointer to the newly allocated `DirectoryScanner`.

### `free_directory_scanner`

Frees a previously allocated `DirectoryScanner`.

- **Parameters**:
  - `scanner_ptr`: Pointer to the `DirectoryScanner` to free.

### `scan_directory_async`

Initiates an asynchronous scan of a directory.

- **Parameters**:
  - `scanner_ptr`: Pointer to an instance of `DirectoryScanner`.
  - `path_ptr`: Path of the directory to scan.

### `get_directory_map`

Retrieves the scanned directory hierarchy as a JSON string.

- **Parameters**:
  - `scanner_ptr`: Pointer to an instance of `DirectoryScanner`.
  - `path_ptr`: Path of the directory to retrieve.
  - `depth`: The depth to which the directory map should be retrieved.

### `stop_scanning`

Requests the ongoing scanning process to stop.

- **Parameters**:
  - `scanner_ptr`: Pointer to an instance of `DirectoryScanner`.


## Testing

The repository includes tests that cover the basic functionality of the DLL:

- **`test_scan_and_get_directory_map`**: Verifies that the scanning function accurately constructs the folder hierarchy and checks if the directory map retrieval function provides the correct JSON structure based on the specified depth.

To run the test, use the following command:

```bash
cargo test
```

## Usage

To use this DLL in your application, follow these steps:

1. **Compile the DLL**: Run `cargo build --release` to compile the code into a DLL named `directory_scanner.dll`.
2. **Include the DLL**: Include the compiled `directory_scanner.dll` file in your project directory.
3. **Link to the DLL**: If using from a C/C++ application, link against the generated `.lib` file. For other languages, use the appropriate method to load and call functions from a DLL.

### Example in C++

Here's an example of how you might call these functions from a C++ application:

```cpp
#include <iostream>
#include <dlfcn.h> // For dynamic loading

// Opaque pointer to DirectoryScanner as we can't directly manipulate it from C++.
typedef void* DirectoryScannerPtr;

// Function pointer types
typedef DirectoryScannerPtr (*CreateDirectoryScannerFn)();
typedef void (*FreeDirectoryScannerFn)(DirectoryScannerPtr scanner_ptr);
typedef void (*ScanDirectoryAsyncFn)(DirectoryScannerPtr scanner_ptr, const char* path_ptr);
typedef char* (*GetDirectoryMapFn)(DirectoryScannerPtr scanner_ptr, const char* path_ptr, int depth);
typedef void (*StopScanningFn)(DirectoryScannerPtr scanner_ptr);

int main() {
    // Load the DLL
    HMODULE hGetProcIDDLL = LoadLibrary("path_to_your_dll.dll");

    // Resolve function addresses
    CreateDirectoryScannerFn createDirectoryScanner = (CreateDirectoryScannerFn)GetProcAddress(hGetProcIDDLL, "create_directory_scanner");
    FreeDirectoryScannerFn freeDirectoryScanner = (FreeDirectoryScannerFn)GetProcAddress(hGetProcIDDLL, "free_directory_scanner");
    ScanDirectoryAsyncFn scanDirectoryAsync = (ScanDirectoryAsyncFn)GetProcAddress(hGetProcIDDLL, "scan_directory_async");
    GetDirectoryMapFn getDirectoryMap = (GetDirectoryMapFn)GetProcAddress(hGetProcIDDLL, "get_directory_map");
    StopScanningFn stopScanning = (StopScanningFn)GetProcAddress(hGetProcIDDLL, "stop_scanning");

    // Initialize the scanner
    void* scanner = createDirectoryScanner();

    // Path to the directory you want to scan
    const char* path = "C:\\path\\to\\scan";

    // Start scanning
    scanDirectoryAsync(scanner, path);

    // ... perform operations ...

    // Stop scanning
    stopScanning(scanner);

    // Retrieve the directory map
    char* directoryMapJson = getDirectoryMap(scanner, path, 0);
    std::cout << "Directory Map: " << directoryMapJson << std::endl;

    // Free the directory scanner
    freeDirectoryScanner(scanner);

    // Clean up
    FreeLibrary(hGetProcIDDLL);

    return 0;
}

```

### Example in TypeScript

To use `directory_scanner.dll` in a TypeScript application, you'll need Node.js and the `ffi-napi` package:

```typescript
import { Library, ref } from 'ffi-napi';

const lib = Library('path_to_your_dll', {
  'create_directory_scanner': ['pointer', []],
  'free_directory_scanner': ['void', ['pointer']],
  'scan_directory_async': ['void', ['pointer', 'string']],
  'get_directory_map': ['string', ['pointer', 'string', 'int']],
  'stop_scanning': ['void', ['pointer']],
});

// Initialize the scanner
const scanner = lib.create_directory_scanner();

const path = 'C:\\path\\to\\scan';

// Start scanning
lib.scan_directory_async(scanner, path);

// ... perform operations ...

// Stop scanning
lib.stop_scanning(scanner);

// Retrieve the directory map
const depth = 0;  // Specify the depth you want
const directoryMapJson: string = lib.get_directory_map(scanner, path, depth);
console.log("Directory Map: ", directoryMapJson);

// Free the directory scanner
lib.free_directory_scanner(scanner);

```

### Example Outputs

Here's what you might expect as output from the get_directory_map function when depth is set to 0 (first level):

```bash
{
    "value": 43,
    "name": ".tmpiEtJbP",
    "path": "C:\\Users\\user\\AppData\\Local\\Temp\\.tmpiEtJbP",
    "children": [
        {
            "value": 27,
            "name": "subfolder1",
            "path": "C:\\Users\\user\\AppData\\Local\\Temp\\.tmpiEtJbP",
            "children": []
        },
        {
            "value": 16,
            "name": "subfolder2",
            "path": "C:\\Users\\user\\AppData\\Local\\Temp\\.tmpiEtJbP",
            "children": []
        }
    ]
}
```
When depth is set to 1 (entire map), the output will include the entire folder hierarchy.

### Contributing

Contributions are welcome! If you have a bug to report or a feature to suggest, please open an issue or a pull request.


**Important Notes:**
- **C++**: The C++ example assumes you have the appropriate function declarations from the DLL. Ensure the function names match exactly what you've exported from Rust.
- **TypeScript**: The TypeScript example uses `ffi-napi` for FFI. Ensure you have installed the package and its types, and that the function names and signatures match your Rust exports.
- **Error Handling and Safety**: Both examples omit error handling for brevity. In a real application, ensure you handle potential errors, such as failed DLL loading, failed function resolution, invalid pointers, etc. Also, ensure that memory management, especially for the Rust-allocated strings, is handled correctly to prevent leaks.
- **Testing**: Before deploying in a production environment, thoroughly test the interaction between your Rust code and the host application in a controlled setting.

