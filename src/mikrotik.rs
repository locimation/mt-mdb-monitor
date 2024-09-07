use std::{collections::HashMap, sync::Arc};

use mikrotik_rs::{command::{response::CommandResponse, CommandBuilder}, MikrotikDevice};
use anyhow::Result;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct BridgeMdbEntry {
    pub group: String,
    pub vlan_id: u32,
    pub ports: Vec<String>,
}

impl TryFrom<&CommandResponse> for BridgeMdbEntry {
    type Error = anyhow::Error;

    fn try_from(response: &CommandResponse) -> Result<Self> {
      if let CommandResponse::Reply(response) = response {
        let group = response.attributes.get("group").ok_or(anyhow::anyhow!("group not found"))?.clone().ok_or(anyhow::anyhow!("group not found"))?;
        let vlan_id = response.attributes.get("vid").ok_or(anyhow::anyhow!("vlan not found"))?.clone().ok_or(anyhow::anyhow!("vlan not found"))?.parse()?;
        let ports = response.attributes.get("on-ports").ok_or(anyhow::anyhow!("vlan not found"))?.clone().ok_or(anyhow::anyhow!("vlan not found"))?.split(',').map(|s| s.to_string()).collect();
        Ok(Self { group, vlan_id, ports })
      } else {
        Err(anyhow::anyhow!("Invalid response type"))
      }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BridgeMdb {
    pub entries: HashMap<String, BridgeMdbEntry>,
}

pub struct MikrotikBridge {
    device: MikrotikDevice,
    mdb: Arc<Mutex<BridgeMdb>>,
    event_tx: tokio::sync::mpsc::Sender<MikrotikBridgeEvent>,
}

pub struct PortMembership {
    pub port: String,
    pub group: String,
    pub vlan_id: u32,
}

pub enum MikrotikBridgeEvent {
    BridgeMdbTableChanged,
    MdbJoin(PortMembership),
    MdbLeave(PortMembership),
}

impl MikrotikBridge {

    pub async fn new(ip: &str, username: &str, password: Option<&str>, quiet_start: bool) -> Result<(Self, tokio::sync::mpsc::Receiver<MikrotikBridgeEvent>)> {

        let (event_tx, event_rx) = tokio::sync::mpsc::channel(10000);

        let device = MikrotikDevice::connect(ip, username, password).await?;
        let device = Self { device, mdb: Default::default(), event_tx };

        device.load_initial_mdb_entries(quiet_start).await?;
        device.listen_bridge_mdb_entries().await?;

        Ok((device, event_rx))

    }

    async fn load_initial_mdb_entries(&self, quiet_start: bool) -> Result<()> {
        let command = CommandBuilder::new().command("/interface/bridge/mdb/print").build();
        let mut response_channel = self.device.send_command(command).await;
        let mut entries = HashMap::new();
        while let Some(Ok(CommandResponse::Reply(response))) = response_channel.recv().await {
            let entry_id = response.attributes.get(".id").ok_or(anyhow::anyhow!("id not found"))?.clone().ok_or(anyhow::anyhow!("id not found"))?;
            let entry = BridgeMdbEntry::try_from(&CommandResponse::Reply(response))?;
            entries.insert(entry_id, entry);
        }

        {
            let mut mdb = self.mdb.lock().await;
            mdb.entries = entries.clone();
        }

        if quiet_start {
            return Ok(());
        }

        for entry in entries.values() {
            for port in entry.ports.iter() {
                self.event_tx.send(MikrotikBridgeEvent::MdbJoin(PortMembership { port: port.clone(), group: entry.group.clone(), vlan_id: entry.vlan_id })).await?;
            }
        }

        self.event_tx.send(MikrotikBridgeEvent::BridgeMdbTableChanged).await?;

        Ok(())
    }

    async fn listen_bridge_mdb_entries(&self) -> Result<()> {
        let command = CommandBuilder::new().command("/interface/bridge/mdb/listen").build();
        let mut response_channel = self.device.send_command(command).await;
        let mdb_ref = self.mdb.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
              while let Some(Ok(CommandResponse::Reply(response))) = response_channel.recv().await {
                if let Some(Some(entry_id)) = response.attributes.clone().get(".id") {
                  if let Some(Some(dead)) = response.attributes.clone().get(".dead") {
                    if dead == "true" {
                      {
                        let mut mdb = mdb_ref.lock().await;

                        // Get the old entry
                        if let Some(old_entry) = mdb.entries.remove(entry_id) {

                          // Send the events
                          for port in old_entry.ports.clone() {
                            if let Err(_) = event_tx.send(MikrotikBridgeEvent::MdbLeave(PortMembership { port, group: old_entry.group.clone(), vlan_id: old_entry.vlan_id })).await {
                              return;
                            }
                          }

                          if let Err(_) = event_tx.send(MikrotikBridgeEvent::BridgeMdbTableChanged).await {
                            return;
                          }

                        }

                      }
                    }
                  } else if let Ok(entry) = BridgeMdbEntry::try_from(&CommandResponse::Reply(response)) {
                    {
                      let mut mdb = mdb_ref.lock().await;

                      // Get the old entry
                      let old_ports = if let Some(old_entry) = mdb.entries.get(entry_id) {
                        old_entry.ports.clone()
                      } else {
                        vec![]
                      };

                      // Insert the new entry
                      mdb.entries.insert(entry_id.to_string(), entry.clone());

                      // Get the new ports
                      let new_ports = entry.ports.clone();

                      // Get the ports that are in the old entry but not in the new entry
                      let ports_no_longer_present = old_ports.iter().filter(|port| !new_ports.contains(port)).map(|port| PortMembership { port: port.clone(), group: entry.group.clone(), vlan_id: entry.vlan_id }).collect::<Vec<_>>();

                      // Get the ports that are in the new entry but not in the old entry
                      let ports_newly_present = new_ports.iter().filter(|port| !old_ports.contains(port)).map(|port| PortMembership { port: port.clone(), group: entry.group.clone(), vlan_id: entry.vlan_id }).collect::<Vec<_>>();

                      // Send the events
                      for port in ports_no_longer_present {
                        if let Err(_) = event_tx.send(MikrotikBridgeEvent::MdbLeave(port)).await {
                          return;
                        }
                      }
                      for port in ports_newly_present {
                        if let Err(_) = event_tx.send(MikrotikBridgeEvent::MdbJoin(port)).await {
                          return;
                        }
                      }

                    }
                    let _ = event_tx.send(MikrotikBridgeEvent::BridgeMdbTableChanged).await;
                  }
                }
              }
            }
        });
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_bridge_mdb_entries(&self) -> Result<HashMap<String, BridgeMdbEntry>> {
        let mdb = self.mdb.lock().await;
        Ok(mdb.entries.clone())
    }

}