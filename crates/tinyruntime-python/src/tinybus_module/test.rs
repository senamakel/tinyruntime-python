//! Tests for the module adapter and its declared surface.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface, Result as TinyBusResult};

use tinyruntime_bus::{
    CONTRACT_VERSION, Language, LayoutRequest, LayoutResponse, ProviderDescriptor, RuntimeSettings,
    WorkerHarness, names,
};

use super::{DEFAULT_VERSION, PythonProvider, setup};

fn provider() -> PythonProvider {
    PythonProvider {
        client: reqwest::Client::new(),
    }
}

/// Start a broker, serve the provider, and return the client proxy to it.
///
/// The module connection is returned alongside because dropping it disconnects
/// the peer, taking the well-known name with it.
async fn serving(bus: &MemoryBus) -> TinyBusResult<(Connection, tinybus::Proxy)> {
    let module = Connection::connect(bus.connect().await?).await?;
    setup(module.clone()).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(
        names::providers::PYTHON,
        names::providers::PYTHON_OBJECT_PATH,
        names::PROVIDER_INTERFACE,
    )?;
    Ok((module, proxy))
}

fn bus() -> MemoryBus {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());
    bus
}

#[test]
fn declared_methods_match_the_dispatch_table() {
    let methods = provider()
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert_eq!(methods, names::PROVIDER_METHODS.to_vec());
}

#[test]
fn the_object_path_is_the_one_the_manifest_will_declare() {
    // `tinybus_module!` derives this module's manifest path from its bus name.
    // Serving anywhere else ships a manifest that disagrees with the object
    // actually exported, which no amount of in-process testing would catch.
    assert_eq!(
        names::providers::PYTHON_OBJECT_PATH,
        names::object_path_for(names::providers::PYTHON)
    );
}

#[test]
fn the_served_interface_is_the_shared_provider_interface() {
    // Serving anything else would make this module unroutable: the router
    // addresses every provider through one interface.
    assert_eq!(provider().name().to_string(), names::PROVIDER_INTERFACE);
}

#[test]
fn the_default_floor_is_a_version() {
    assert!(crate::parse_version(DEFAULT_VERSION).is_some());
}

#[tokio::test]
async fn the_router_can_describe_this_provider_over_a_bus() -> TinyBusResult<()> {
    let bus = bus();
    let (_module, proxy) = serving(&bus).await?;

    let descriptor: ProviderDescriptor = proxy.call(names::provider_methods::DESCRIBE, ()).await?;
    assert_eq!(descriptor.language, Language::python());
    assert_eq!(descriptor.display_name, "Python");
    assert_eq!(
        descriptor.contract_version, CONTRACT_VERSION,
        "the router refuses a provider it cannot bind to"
    );
    assert!(descriptor.executables.contains(&"pip".to_string()));
    Ok(())
}

#[tokio::test]
async fn the_harness_crosses_the_bus_intact() -> TinyBusResult<()> {
    // The harness is a script the router writes out and launches; if it did not
    // survive the round trip, every worker would fail at its handshake.
    let bus = bus();
    let (_module, proxy) = serving(&bus).await?;

    let harness: WorkerHarness = proxy.call(names::provider_methods::HARNESS, ()).await?;
    assert_eq!(harness, crate::harness());
    assert!(!harness.source.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_directory_that_is_not_an_install_is_reported_empty_rather_than_failing()
-> TinyBusResult<()> {
    // The router scans a cache full of directories that are not installs. If
    // this failed instead of answering, one leftover would break every scan.
    let bus = bus();
    let (_module, proxy) = serving(&bus).await?;

    let scratch = tempfile::tempdir().expect("scratch directory");
    let response: LayoutResponse = proxy
        .call(
            names::provider_methods::LAYOUT,
            (LayoutRequest::new(
                scratch.path().to_string_lossy(),
                RuntimeSettings::new(DEFAULT_VERSION),
            ),),
        )
        .await?;

    assert!(response.layout.is_none());
    Ok(())
}

#[tokio::test]
async fn detecting_a_host_interpreter_answers_rather_than_failing() -> TinyBusResult<()> {
    // Whether this machine has Python is not the point: the call must complete
    // either way, because "nothing here" is how the router learns to install.
    let bus = bus();
    let (_module, proxy) = serving(&bus).await?;

    let _: LayoutResponse = proxy
        .call(
            names::provider_methods::DETECT_SYSTEM,
            (RuntimeSettings::new(DEFAULT_VERSION),),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_floor_that_is_not_a_version_is_refused_with_a_readable_reason() -> TinyBusResult<()> {
    let bus = bus();
    let (_module, proxy) = serving(&bus).await?;

    let result = proxy
        .call::<tinyruntime_bus::Distribution>(
            names::provider_methods::SELECT_DISTRIBUTION,
            (RuntimeSettings::new("latest"),),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed("`latest` unexpectedly resolved"));
    };
    let rendered = error.to_string();
    assert!(rendered.contains("latest"), "got `{rendered}`");
    Ok(())
}
