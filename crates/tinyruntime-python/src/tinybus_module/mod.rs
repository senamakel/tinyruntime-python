//! The `TinyBus` module entrypoint and the provider interface.
//!
//! This module answers the router's five questions and does nothing else. It
//! downloads no archive, unpacks nothing, writes nothing to a cache, and starts
//! no worker — all of that belongs to the router, which does it identically for
//! every language.
//!
//! What is left is exactly the Python knowledge: which host interpreters count,
//! which standalone build to install from a moving release index, where the
//! interpreter sits inside it, and what a warm Python worker is.
//!
//! The interface it implements is [`names::PROVIDER_INTERFACE`], the same one
//! every provider implements — that is what makes them interchangeable. The
//! well-known name it claims is its own, because two peers cannot hold the same
//! one, and it serves at the path derived from that name: `tinybus_module!`
//! builds this module's manifest path the same way, so serving anywhere else
//! would ship a manifest that disagreed with the object exported here.

use std::path::Path;

use reqwest::Client;
use tinybus::{Connection, Result as TinyBusResult};

use tinyruntime_bus::{
    Distribution, Language, LayoutRequest, LayoutResponse, ProviderDescriptor, RuntimeSettings,
    WorkerHarness, names,
};

use crate::{distribution, harness, layout, system};

/// The version floor this provider targets when a host expresses no preference.
///
/// A floor rather than a pin, because that is what the standalone channel
/// supports: it publishes a moving set of builds, and an exact patch would stop
/// resolving the moment that build rotated out.
pub const DEFAULT_VERSION: &str = "3.12";

/// The object this module serves.
struct PythonProvider {
    client: Client,
}

#[tinybus::interface(name = "ai.tinyhumans.runtime.Provider")]
impl PythonProvider {
    /// What this provider is and what it targets by default.
    async fn describe(&self) -> TinyBusResult<ProviderDescriptor> {
        let mut descriptor = ProviderDescriptor::new(Language::python(), "Python", DEFAULT_VERSION);
        for tool in layout::TOOLS {
            descriptor = descriptor.with_executable(*tool);
        }
        std::future::ready(Ok(descriptor)).await
    }

    /// Look for a compatible interpreter already on this host.
    async fn detect_system(&self, settings: RuntimeSettings) -> TinyBusResult<LayoutResponse> {
        Ok(system::detect(&settings)
            .await
            .map_or_else(LayoutResponse::missing, LayoutResponse::found))
    }

    /// Pick the standalone build to install.
    async fn select_distribution(&self, settings: RuntimeSettings) -> TinyBusResult<Distribution> {
        distribution::select(&self.client, &settings)
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))
    }

    /// Report where the interpreter is inside an install the router unpacked.
    async fn layout(&self, request: LayoutRequest) -> TinyBusResult<LayoutResponse> {
        Ok(
            layout::describe(Path::new(&request.install_dir), &request.settings)
                .await
                .map_or_else(LayoutResponse::missing, LayoutResponse::found),
        )
    }

    /// Supply the warm-worker harness for this language.
    async fn harness(&self) -> TinyBusResult<WorkerHarness> {
        std::future::ready(Ok(harness::harness())).await
    }
}

/// Start serving the provider interface.
async fn setup(connection: Connection) -> TinyBusResult<()> {
    connection
        .serve_at(
            names::providers::PYTHON_OBJECT_PATH.try_into()?,
            PythonProvider {
                client: Client::new(),
            },
        )
        .await?;
    connection.request_name(names::providers::PYTHON).await?;
    tracing::info!("[tinyruntime-python] serving the python runtime provider");
    Ok(())
}

tinybus_module::module_export! {
    setup = setup,
    worker_threads = 1,
    provides = ["ai.tinyhumans.runtime.python.Provider"],
    methods = ["Describe", "DetectSystem", "SelectDistribution", "Layout", "Harness"],
    signals = [],
    requires = [],
    optional = [],
    lazy = false,
}

#[cfg(test)]
mod test;
