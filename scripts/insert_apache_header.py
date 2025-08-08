import os

# Apache 2.0 license header in Rust-style comments
APACHE_HEADER = """\
// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

"""

def has_apache_header(content: str) -> bool:
    """Check if the Apache license is already present in the file."""
    return "Licensed under the Apache License, Version 2.0" in content

def find_insert_index(lines: list[str]) -> int:
    """
    Determine the correct position to insert the license header.
    Skip lines starting with #![...] or //!, which must stay at the top.
    """
    index = 0
    while index < len(lines):
        line = lines[index].strip()
        if line.startswith("#!") or line.startswith("//!"):
            index += 1
        elif line == "":
            index += 1  # Also skip leading empty lines
        else:
            break
    return index

def insert_header_in_file(path: str):
    """Insert the Apache license header into a given Rust file, if not already present."""
    with open(path, 'r', encoding='utf-8') as f:
        lines = f.readlines()

    content = "".join(lines)
    if has_apache_header(content):
        print(f"✅ Already has header: {path}")
        return

    insert_index = find_insert_index(lines)
    new_lines = lines[:insert_index] + [APACHE_HEADER] + lines[insert_index:]

    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(new_lines)

    print(f"📝 Header inserted: {path}")

def process_directory(root_dir: str):
    """
    Recursively find all `.rs` files under the given directory
    and insert the license header where needed.
    """
    for root, dirs, files in os.walk(root_dir):
        # Skip unnecessary directories
        dirs[:] = [d for d in dirs if d not in ('target', '.git')]
        for filename in files:
            if filename.endswith('.rs'):
                filepath = os.path.join(root, filename)
                insert_header_in_file(filepath)

if __name__ == "__main__":
    target_dir = "."  # Start from the current directory
    process_directory(target_dir)