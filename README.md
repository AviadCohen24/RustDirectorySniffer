# Rust Folder Hierarchy DLL

This repository contains a Rust-compiled Dynamic Link Library (DLL) named `directory_scanner.dll`, designed for efficient folder hierarchy scanning and manipulation. It provides fast and reliable access to file system operations, particularly suited for applications requiring extensive directory traversal and organization.

## Features

- **Scan Directory**: Recursively scans directories and constructs a hierarchical structure of folders and files.
- **Get Directory Map**: Retrieves the hierarchical structure of a specified directory. Users can choose to retrieve only the first level or the entire directory map.

## Functions

### `scan_directory_async`
- **Description**: Scans the directory asynchronously and updates the global directory map with the folder hierarchy.
- **Arguments**: 
  - `path_ptr`: A pointer to the directory path (as a C-style string) to scan.
- **Returns**: Nothing. Updates are made to the global state.

### `get_directory_map`
- **Description**: Retrieves the folder hierarchy for a specified directory path. Users can specify the depth of detail in the returned data.
- **Arguments**: 
  - `path_ptr`: A pointer to the directory path (as a C-style string) whose hierarchy is to be retrieved.
  - `depth`: An integer where `0` indicates only the first level should be returned and `1` indicates the entire map should be returned.
- **Returns**: A pointer to a C-style string containing the JSON representation of the folder hierarchy.

## Testing

The repository includes tests that cover the basic functionality of the DLL:

- **`test_get_folder_size`**: Ensures the function correctly calculates the size of a folder.
- **`test_scan_folder`**: Verifies that the scanning function accurately constructs the folder hierarchy.
- **`test_get_directory_map`**: Checks if the directory map retrieval function provides the correct JSON structure based on the specified depth.

To run the tests, use the following command:

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
// Load the DLL
HMODULE hModule = LoadLibrary(L"directory_scanner.dll");

// Get function pointers
auto scan_directory_async = (void(*)(const char*))GetProcAddress(hModule, "scan_directory_async");
auto get_directory_map = (char*(*)(const char*, int))GetProcAddress(hModule, "get_directory_map");

// Use the functions
scan_directory_async("C:\\path\\to\\directory");
char* json_hierarchy = get_directory_map("C:\\path\\to\\directory", 0);  // 0 for first level, 1 for full map

// Process the JSON hierarchy
// ...

// Free the DLL
FreeLibrary(hModule);
```

### Example in TypeScript

To use `directory_scanner.dll` in a TypeScript application, you'll need Node.js and the `ffi-napi` package:

```typescript
import { Library, Function } from 'ffi-napi';

// Define the types for your functions
const directoryScanner = new Library('directory_scanner.dll', {
  'scan_directory_async': ['void', ['string']],
  'get_directory_map': ['string', ['string', 'int']]
});

// Call the functions
directoryScanner.scan_directory_async("C:\\path\\to\\directory");
const jsonHierarchy: string = directoryScanner.get_directory_map("C:\\path\\to\\directory", 0);  // 0 for first level, 1 for full map

console.log("Directory Hierarchy:", jsonHierarchy);
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
