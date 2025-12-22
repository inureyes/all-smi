// Copyright 2025 Lablup Inc.
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only compile proto files on Linux (TPU is Linux-only)
    #[cfg(target_os = "linux")]
    {
        let proto_file = "proto/tpu_metric_service.proto";

        // Check if proto file exists before trying to compile
        if std::path::Path::new(proto_file).exists() {
            tonic_build::configure()
                .build_server(false) // We only need the client
                .protoc_arg("--experimental_allow_proto3_optional")
                .compile_protos(&[proto_file], &["proto/"])?;
        }
    }

    Ok(())
}
