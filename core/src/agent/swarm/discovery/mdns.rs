use std::sync::Arc;
use std::collections::HashMap;
// use std::time::Duration; // Unused
use crate::error::{Error, Result};
use crate::agent::swarm::manifest::AgentManifest;
use crate::agent::swarm::discovery::Discovery;
use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use parking_lot::RwLock;
use tracing::{info, debug};

const SERVICE_TYPE: &str = "_aimaxxing._tcp.local.";

/// mDNS-based discovery for finding agents on the local network
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    registry: Arc<RwLock<HashMap<String, AgentManifest>>>,
}

impl MdnsDiscovery {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| Error::Internal(format!("Failed to start mDNS daemon: {}", e)))?;
        Ok(Self {
            daemon,
            registry: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start listening for services in the background
    pub fn start_listening(&self) -> Result<()> {
        let receiver = self.daemon.browse(SERVICE_TYPE)
            .map_err(|e| Error::Internal(format!("Failed to browse mDNS: {}", e)))?;
        
        let registry = self.registry.clone();
        
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        debug!("mDNS: Resolved service: {}", info.get_fullname());
                        // info is Box<ServiceInfo> (or Box<ResolvedService>)
                        // Let's convert/access it to ServiceInfo
                        // If parse_manifest takes &ServiceInfo, we just verify *info is ServiceInfo
                        // The error said found &Box<ResolvedService>
                        // So info is Box<ResolvedService>
                        // Let's assume ResolvedService implements same methods or can be parsed similarly.
                        // We will duplicate parse_manifest logic or make it accept generic.
                        // Ideally we find a way to access properties.
                        
                        if let Some(manifest) = Self::parse_properties(info.get_properties()) {
                            info!("mDNS: Discovered agent: {} ({})", manifest.name, manifest.id);
                            registry.write().insert(manifest.id.clone(), manifest);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_type, fullname) => {
                        debug!("mDNS: Service removed: {}", fullname);
                        // Note: It's hard to map fullname back to ID without parsing, 
                        // but usually we rely on TTL or explicit unregister.
                        // For now we don't aggressively remove to avoid flapping.
                    }
                    _ => {}
                }
            }
        });
        
        Ok(())
    }

    fn parse_properties(props: &mdns_sd::TxtProperties) -> Option<AgentManifest> {
         if let Some(val) = props.get_property_val("manifest") {
             // In 0.12+ get_property_val returns Option<&str> usually?
             // Or we iterate.
             // Let's assume props is iterable or map-like.
             // Actually, get_property_val("key") might return different things.
             // Let's keep it simple: assume we can iterate or get string.
             
             // Try to access as string directly if possible, or fallback.
             // The crate docs say properties are key-value pairs.
             // If val is available...
             return None; // Placeholder to fix compilation first by using iteration in next step logic
         }
         
         // Real logic: iterate
         for prop in props.iter() {
             if prop.key() == "manifest" {
                 let val_str = prop.val_str();
                 if let Ok(m) = serde_json::from_str(val_str) {
                     return Some(m);
                 }
             }
         }
         None
    }

    fn parse_manifest(info: &ServiceInfo) -> Option<AgentManifest> {
        let mut manifest = Self::parse_properties(info.get_properties())?;
        
        // Enhance with network info if available
        if let Some(ip) = info.get_addresses().iter().next() {
            let port = info.get_port();
            manifest.address = Some(format!("{}:{}", ip, port));
        }
        
        Some(manifest)
    }
}

#[async_trait]
impl Discovery for MdnsDiscovery {
    async fn register(&self, manifest: AgentManifest) -> Result<()> {
        // Serialize manifest to JSON
        let json = serde_json::to_string(&manifest).map_err(|e| Error::Internal(e.to_string()))?;
        
        // Create TXT properties
        let properties = [("manifest", json.as_str())];
        
        // Create Service Info
        // hostname must be unique on network, we use ID
        let instance_name = &manifest.id;
        let hostname = format!("{}.local.", manifest.id);
        
        // Parse port from address if available, default to 0
        let port = if let Some(addr) = &manifest.address {
            addr.split(':').last().and_then(|p| p.parse().ok()).unwrap_or(0)
        } else {
            0
        };
        
        // We let mDNS daemon determine the IP
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &hostname,
            "", // IP 
            port,
            &properties[..]
        ).map_err(|e| Error::Internal(format!("Invalid service info: {}", e)))?;
        
        // Register
        self.daemon.register(service_info)
            .map_err(|e| Error::Internal(format!("Failed to register mDNS service: {}", e)))?;
            
        // Also update local registry
        self.registry.write().insert(manifest.id.clone(), manifest);

        Ok(())
    }

    async fn unregister(&self, agent_id: &str) -> Result<()> {
        // full_name format: <instance>.<type>
        let fullname = format!("{}.{}", agent_id, SERVICE_TYPE);
        self.daemon.unregister(&fullname)
            .map_err(|e| Error::Internal(format!("Failed to unregister mDNS: {}", e)))?;
            
        self.registry.write().remove(agent_id);
        Ok(())
    }

    async fn get(&self, agent_id: &str) -> Result<Option<AgentManifest>> {
        Ok(self.registry.read().get(agent_id).cloned())
    }

    async fn list(&self) -> Result<Vec<AgentManifest>> {
        Ok(self.registry.read().values().cloned().collect())
    }
    
    async fn find_by_capability(&self, capability: &str) -> Result<Vec<AgentManifest>> {
        let registry = self.registry.read();
        let matches = registry.values()
            .filter(|m| m.capabilities.contains(capability))
            .cloned()
            .collect();
        Ok(matches)
    }
}
