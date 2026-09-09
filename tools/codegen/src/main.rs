//! Explicit developer tool; never invoked by the component's normal build.
use prost::Message as _;
use prost_types::{
    DescriptorProto, FileDescriptorSet,
    field_descriptor_proto::{Label, Type},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::Write as _,
    fs,
    path::Path,
    process::Command,
};

const ROOTS: [(&str, &str); 6] = [
    ("START_REQUEST", "StartWorkflowExecutionRequest"),
    ("START_RESPONSE", "StartWorkflowExecutionResponse"),
    ("DESCRIBE_REQUEST", "DescribeWorkflowExecutionRequest"),
    ("DESCRIBE_RESPONSE", "DescribeWorkflowExecutionResponse"),
    ("HISTORY_REQUEST", "GetWorkflowExecutionHistoryRequest"),
    ("HISTORY_RESPONSE", "GetWorkflowExecutionHistoryResponse"),
];
const PACKAGE: &str = ".temporal.api.workflowservice.v1";

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: temporal-protobuf-codegen VENDORED_PROTO_ROOT OUTPUT_DIRECTORY".into());
    }
    let vendor = Path::new(&args[1]).canonicalize()?;
    let output = Path::new(&args[2]);
    fs::create_dir_all(output)?;
    let output = output.canonicalize()?;
    let protoc = std::env::var_os("PROTOC").unwrap_or_else(|| "protoc".into());
    let version = Command::new(&protoc).arg("--version").output()?;
    if !version.status.success() || version.stdout != b"libprotoc 35.1\n" {
        return Err("regeneration requires exactly libprotoc 35.1".into());
    }
    let descriptor_path = output.join("upstream-descriptor.pb");
    let status = Command::new(&protoc)
        .arg(format!("--proto_path={}", vendor.display()))
        .arg(format!(
            "--descriptor_set_out={}",
            descriptor_path.display()
        ))
        .arg("--include_imports")
        .arg("temporal/api/workflowservice/v1/service.proto")
        .status()?;
    if !status.success() {
        return Err("protoc descriptor generation failed".into());
    }
    let mut descriptors = FileDescriptorSet::decode(fs::read(descriptor_path)?.as_slice())?;
    // Also reject implicit protoc/system include fallback: every descriptor must
    // have an explicit source in the hash-inventoried vendored directory.
    for file in &descriptors.file {
        if !vendor.join(file.name()).is_file() {
            return Err(format!("non-vendored import: {}", file.name()).into());
        }
    }
    prune(&mut descriptors)?;
    fs::write(
        output.join("messages-descriptor.pb"),
        descriptors.encode_to_vec(),
    )?;
    let mut config = prost_build::Config::new();
    config
        .out_dir(&output)
        .include_file("mod.rs")
        .btree_map(["."]);
    // There is deliberately no ServiceGenerator, gRPC client, build.rs, or
    // runtime descriptor parser. Only the six message roots are retained.
    config.compile_fds(descriptors.clone())?;
    fs::write(output.join("wire.rs"), wire_schema(&descriptors)?)?;
    println!("generated six message roots with prost-build 0.14.4 and libprotoc 35.1");
    Ok(())
}

fn index_message(
    message: &DescriptorProto,
    prefix: &str,
    owner: &str,
    owners: &mut BTreeMap<String, String>,
) {
    let name = format!("{prefix}.{}", message.name());
    owners.insert(name.clone(), owner.to_owned());
    for child in &message.nested_type {
        index_message(child, &name, owner, owners);
    }
    for enumeration in &message.enum_type {
        owners.insert(format!("{name}.{}", enumeration.name()), owner.to_owned());
    }
}

fn references(message: &DescriptorProto, refs: &mut BTreeSet<String>) {
    for field in &message.field {
        if let Some(name) = &field.type_name {
            refs.insert(name.clone());
        }
    }
    for child in &message.nested_type {
        references(child, refs);
    }
}

fn prune(descriptors: &mut FileDescriptorSet) -> Result<(), Box<dyn Error>> {
    let mut owners = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    for file in &descriptors.file {
        let package = format!(".{}", file.package());
        for message in &file.message_type {
            let name = format!("{package}.{}", message.name());
            index_message(message, &package, &name, &mut owners);
            let mut refs = BTreeSet::new();
            references(message, &mut refs);
            dependencies.insert(name, refs);
        }
        for enumeration in &file.enum_type {
            let name = format!("{package}.{}", enumeration.name());
            owners.insert(name.clone(), name.clone());
            dependencies.insert(name, BTreeSet::new());
        }
    }
    let mut needed = BTreeSet::new();
    let mut pending: Vec<_> = ROOTS
        .iter()
        .map(|(_, name)| format!("{PACKAGE}.{name}"))
        .collect();
    while let Some(name) = pending.pop() {
        let owner = owners
            .get(&name)
            .ok_or_else(|| format!("unknown message dependency {name}"))?;
        if needed.insert(owner.clone()) {
            pending.extend(
                dependencies
                    .get(owner)
                    .ok_or("missing dependency owner")?
                    .iter()
                    .cloned(),
            );
        }
    }
    for file in &mut descriptors.file {
        let package = format!(".{}", file.package());
        file.message_type
            .retain(|message| needed.contains(&format!("{package}.{}", message.name())));
        file.enum_type
            .retain(|enumeration| needed.contains(&format!("{package}.{}", enumeration.name())));
        file.service.clear();
        file.extension.clear();
        // Index paths no longer address the same declarations after pruning.
        // Do not attach a different upstream declaration's docs to a message.
        file.source_code_info = None;
    }
    Ok(())
}

fn flatten<'a>(
    message: &'a DescriptorProto,
    prefix: &str,
    messages: &mut BTreeMap<String, &'a DescriptorProto>,
) {
    let name = format!("{prefix}.{}", message.name());
    messages.insert(name.clone(), message);
    for child in &message.nested_type {
        flatten(child, &name, messages);
    }
}

fn wire_schema(descriptors: &FileDescriptorSet) -> Result<String, Box<dyn Error>> {
    let mut messages = BTreeMap::new();
    for file in &descriptors.file {
        for message in &file.message_type {
            flatten(message, &format!(".{}", file.package()), &mut messages);
        }
    }
    let indices: BTreeMap<_, _> = messages
        .keys()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect();
    let mut text = String::from(
        "// Generated from the pinned protoc descriptors; do not hand-edit field identities.\n\
#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n\
pub enum Kind { Varint, Fixed64, Bytes, String, Message(usize), Fixed32 }\n\
#[derive(Clone, Copy, Debug)]\n\
pub struct Field { pub name: &'static str, pub number: u32, pub kind: Kind, pub repeated: bool }\n\
#[derive(Clone, Copy, Debug)]\n\
pub struct Message { pub name: &'static str, pub fields: &'static [Field], pub map_entry: bool }\n",
    );
    for (constant, name) in ROOTS {
        writeln!(
            text,
            "pub const {constant}: usize = {};",
            indices[format!("{PACKAGE}.{name}").as_str()]
        )?;
    }
    text.push_str("pub static MESSAGE_SCHEMA: &[Message] = &[\n");
    for (name, message) in &messages {
        writeln!(
            text,
            "Message {{ name: {:?}, map_entry: {}, fields: &[",
            name.trim_start_matches('.'),
            message
                .options
                .as_ref()
                .is_some_and(|options| options.map_entry())
        )?;
        for field in &message.field {
            let kind = match field.r#type() {
                Type::Double | Type::Fixed64 | Type::Sfixed64 => "Kind::Fixed64".to_owned(),
                Type::Float | Type::Fixed32 | Type::Sfixed32 => "Kind::Fixed32".to_owned(),
                Type::String => "Kind::String".to_owned(),
                Type::Bytes => "Kind::Bytes".to_owned(),
                Type::Message => format!(
                    "Kind::Message({})",
                    indices
                        .get(field.type_name())
                        .ok_or("missing nested descriptor")?
                ),
                Type::Group => return Err("groups are outside the pinned proto3 schema".into()),
                _ => "Kind::Varint".to_owned(),
            };
            let number = u32::try_from(field.number())?;
            writeln!(
                text,
                "Field {{ name: {:?}, number: {number}, kind: {kind}, repeated: {} }},",
                field.name(),
                field.label() == Label::Repeated
            )?;
        }
        text.push_str("] },\n");
    }
    text.push_str("];\n");
    Ok(text)
}
