//! Loads a built module through the real `TinyBus` dynamic loader.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use tinybus::Connection;
use tinybus::broker::Broker;
use tinybus::module::ModuleHost;
use tinybus::transport::memory::MemoryBus;
use tinyruntime_python::{ProviderDescriptor, names};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = module_argument()?;
    let bus = MemoryBus::new();
    let broker = Broker::new();
    let broker_task = broker.spawn(bus.clone());
    let module_host = ModuleHost::new(broker);
    let info = module_host.load_file(&module)?;

    if info.name != env!("CARGO_PKG_NAME") {
        return Err(io::Error::other(format!(
            "loaded module `{}` instead of `{}`",
            info.name,
            env!("CARGO_PKG_NAME")
        ))
        .into());
    }

    let client = Connection::connect(bus.connect().await?).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let claimed = client.list_names().await?;
            if claimed
                .iter()
                .any(|name| name.as_str() == names::providers::PYTHON)
            {
                return tinybus::Result::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    // `Describe` is the right probe for a provider: it exercises the whole
    // dispatch path and needs neither a network nor an installed interpreter, so
    // it verifies the artifact rather than the machine it happens to run on.
    let proxy = client.proxy(
        names::providers::PYTHON,
        names::PROVIDER_OBJECT_PATH,
        names::PROVIDER_INTERFACE,
    )?;
    let descriptor: ProviderDescriptor = proxy.call(names::provider_methods::DESCRIBE, ()).await?;
    if descriptor.language.as_str() != tinyruntime_python::PYTHON {
        return Err(io::Error::other(format!(
            "module claims to serve `{}` rather than python",
            descriptor.language
        ))
        .into());
    }

    println!(
        "verified {} as TinyBus module `{}`, providing {} {}",
        module.display(),
        info.name,
        descriptor.display_name,
        descriptor.default_version
    );
    broker_task.abort();
    Ok(())
}

fn module_argument() -> Result<PathBuf, io::Error> {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: cargo run --example verify_module -- <module-path>",
            )
        })
}
