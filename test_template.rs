fn main() {
    let mut template = String::new();
    
    // Simulating what add_disk_metrics does
    template.push_str("# HELP all_smi_disk_total_bytes Total disk space in bytes\n");
    template.push_str("# TYPE all_smi_disk_total_bytes gauge\n");
    let disk_labels = format\!("instance=\"{}\", mount_point=\"/\", index=\"0\"", "test");
    template.push_str(&format\!(
        "all_smi_disk_total_bytes{{{disk_labels}}} {{{{DISK_TOTAL}}}}\n"
    ));
    
    println\!("Template contains: {}", template);
}
