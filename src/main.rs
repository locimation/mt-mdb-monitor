use anyhow::Result;

use mikrotik::{MikrotikBridge, MikrotikBridgeEvent, PortMembership};
use tokio;

use tracing::{info, error};
use tracing_loki::url::Url;
use tracing_subscriber::{self, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;

mod mikrotik;

#[derive(Parser, Debug)]
struct Args {

    // Router's address
    host: String,

    // Router's username
    #[clap(short, long)]
    username: String,

    // Router's password
    #[clap(short, long)]
    password: Option<String>,

    // Grafana Loki address
    #[clap(short, long)]
    loki: Option<String>,

    // No initial logging
    #[clap(short, long)]
    quietstart: bool

}

#[tokio::main]
async fn main() -> Result<()> {

    let args = Args::parse();

    if let Some(loki) = &args.loki {

        match Url::parse(loki) {

            Err(e) => {
                error!("Invalid Loki URL: {}", e);
                std::process::exit(1);
            }

            Ok(url) => {

                let (layer, task) = tracing_loki::builder()
                    .label("app", "mikrotik-mdb")
                    .map_err(|e| anyhow::anyhow!("Failed to create Loki layer: {}", e))?
                    .label("host", args.host.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to create Loki layer: {}", e))?
                    .build_url(url)
                    .map_err(|e| anyhow::anyhow!("Failed to create Loki layer: {}", e))?;

                if let Err(e) = tracing_subscriber::registry()
                    .with(layer)
                    .with(tracing_subscriber::fmt::layer())
                    .try_init()
                {
                    error!("Failed to initialize Loki layer: {}", e);
                }

                tokio::spawn(task);

            }
        }
    } else {
        tracing_subscriber::fmt::init();
    }

    // Router's address with port
    let addr = format!("{}:8728", args.host);

    // Connect to the router
    let (_bridge, mut events_channel) = MikrotikBridge::new(&addr, &args.username, args.password.as_deref(), args.quietstart).await.expect("Failed to connect to Mikrotik router");
    info!("Connected to Mikrotik router on {}", addr);

    // Get bridge mdb entries
    loop {

        let event = events_channel.recv().await.unwrap();
        match event {
            MikrotikBridgeEvent::MdbJoin(PortMembership { port, group, vlan_id }) => {
                info!("Port {} joined group {} in VLAN {}", port, group, vlan_id);
            }
            MikrotikBridgeEvent::MdbLeave(PortMembership { port, group, vlan_id }) => {
                info!("Port {} left group {} in VLAN {}", port, group, vlan_id);
            }
            MikrotikBridgeEvent::BridgeMdbTableChanged => {}
        }

    }

}