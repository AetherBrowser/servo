// components/net/adblock_config.rs
//
// Configuration pour le moteur de blocage de publicités

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration du moteur adblock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdBlockConfig {
    /// Active ou désactive le blocage de publicités
    pub enabled: bool,
    
    /// Chemins vers les listes de filtres locales
    pub filter_lists: Vec<PathBuf>,
    
    /// URLs pour télécharger les listes automatiquement
    pub filter_urls: Vec<FilterListUrl>,
    
    /// Domaines à ne jamais bloquer (whitelist)
    pub whitelist_domains: Vec<String>,
    
    /// Mode agressif (peut bloquer plus de contenu)
    pub aggressive_mode: bool,
    
    /// Mise à jour automatique des filtres
    pub auto_update: bool,
    
    /// Intervalle de mise à jour en heures
    pub update_interval_hours: u64,
}

/// URL de liste de filtres avec métadonnées
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterListUrl {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

impl Default for AdBlockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filter_lists: vec![
                PathBuf::from("resources/adblock/easylist.txt"),
                PathBuf::from("resources/adblock/easyprivacy.txt"),
            ],
            filter_urls: vec![
                FilterListUrl {
                    name: "EasyList".to_string(),
                    url: "https://easylist.to/easylist/easylist.txt".to_string(),
                    enabled: true,
                },
                FilterListUrl {
                    name: "EasyPrivacy".to_string(),
                    url: "https://easylist.to/easylist/easyprivacy.txt".to_string(),
                    enabled: true,
                },
            ],
            whitelist_domains: vec![],
            aggressive_mode: false,
            auto_update: true,
            update_interval_hours: 24,
        }
    }
}

impl AdBlockConfig {
    /// Crée une nouvelle configuration avec les valeurs par défaut
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Charge la configuration depuis un fichier JSON
    /// 
    /// # Arguments
    /// * `path` - Chemin vers le fichier de configuration
    /// 
    /// # Retourne
    /// La configuration chargée ou une erreur
    pub fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config = serde_json::from_str(&content)?;
        Ok(config)
    }
    
    /// Sauvegarde la configuration dans un fichier JSON
    /// 
    /// # Arguments
    /// * `path` - Chemin où sauvegarder la configuration
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        // Créer le dossier parent si nécessaire
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    /// Charge ou crée une configuration par défaut
    /// 
    /// Si le fichier existe, il est chargé. Sinon, une configuration
    /// par défaut est créée et sauvegardée.
    pub fn load_or_default(path: &PathBuf) -> Self {
        match Self::load_from_file(path) {
            Ok(config) => {
                println!("AdBlock: Configuration chargée depuis {:?}", path);
                config
            }
            Err(_) => {
                println!("AdBlock: Création d'une nouvelle configuration");
                let config = Self::default();
                
                // Essayer de sauvegarder la configuration par défaut
                if let Err(e) = config.save_to_file(path) {
                    eprintln!("AdBlock: Impossible de sauvegarder la configuration: {}", e);
                }
                
                config
            }
        }
    }
    
    /// Ajoute un domaine à la whitelist
    pub fn add_whitelist_domain(&mut self, domain: String) {
        if !self.whitelist_domains.contains(&domain) {
            self.whitelist_domains.push(domain);
        }
    }
    
    /// Retire un domaine de la whitelist
    pub fn remove_whitelist_domain(&mut self, domain: &str) {
        self.whitelist_domains.retain(|d| d != domain);
    }
    
    /// Vérifie si un domaine est dans la whitelist
    pub fn is_whitelisted(&self, domain: &str) -> bool {
        self.whitelist_domains.iter().any(|d| domain.contains(d))
    }
    
    /// Active une liste de filtres par nom
    pub fn enable_filter_list(&mut self, name: &str) {
        for filter in &mut self.filter_urls {
            if filter.name == name {
                filter.enabled = true;
            }
        }
    }
    
    /// Désactive une liste de filtres par nom
    pub fn disable_filter_list(&mut self, name: &str) {
        for filter in &mut self.filter_urls {
            if filter.name == name {
                filter.enabled = false;
            }
        }
    }
    
    /// Retourne les URLs de filtres activées
    pub fn get_enabled_filter_urls(&self) -> Vec<String> {
        self.filter_urls
            .iter()
            .filter(|f| f.enabled)
            .map(|f| f.url.clone())
            .collect()
    }
    
    /// Ajoute une nouvelle liste de filtres personnalisée
    pub fn add_custom_filter_list(&mut self, name: String, url: String) {
        self.filter_urls.push(FilterListUrl {
            name,
            url,
            enabled: true,
        });
    }
}

/// Configuration avancée pour des cas d'usage spécifiques
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    /// Bloquer les WebSockets utilisés pour le tracking
    pub block_tracking_websockets: bool,
    
    /// Bloquer les WebRTC pour éviter les fuites d'IP
    pub block_webrtc: bool,
    
    /// Bloquer les requêtes de fingerprinting
    pub block_fingerprinting: bool,
    
    /// Bloquer les cookies tiers
    pub block_third_party_cookies: bool,
    
    /// Cache les décisions pour améliorer les performances
    pub enable_cache: bool,
    
    /// Taille maximale du cache
    pub cache_size: usize,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            block_tracking_websockets: true,
            block_webrtc: false,
            block_fingerprinting: true,
            block_third_party_cookies: false,
            enable_cache: true,
            cache_size: 10000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_default_config() {
        let config = AdBlockConfig::default();
        assert!(config.enabled);
        assert!(!config.aggressive_mode);
        assert_eq!(config.filter_lists.len(), 2);
        assert_eq!(config.filter_urls.len(), 2);
    }
    
    #[test]
    fn test_whitelist() {
        let mut config = AdBlockConfig::default();
        
        config.add_whitelist_domain("example.com".to_string());
        assert!(config.is_whitelisted("example.com"));
        assert!(config.is_whitelisted("subdomain.example.com"));
        
        config.remove_whitelist_domain("example.com");
        assert!(!config.is_whitelisted("example.com"));
    }
    
    #[test]
    fn test_filter_list_management() {
        let mut config = AdBlockConfig::default();
        
        // Ajouter une liste personnalisée
        config.add_custom_filter_list(
            "Custom List".to_string(),
            "https://example.com/filters.txt".to_string()
        );
        
        assert_eq!(config.filter_urls.len(), 3);
        
        // Désactiver EasyList
        config.disable_filter_list("EasyList");
        let enabled = config.get_enabled_filter_urls();
        assert_eq!(enabled.len(), 2);
        
        // Réactiver EasyList
        config.enable_filter_list("EasyList");
        let enabled = config.get_enabled_filter_urls();
        assert_eq!(enabled.len(), 3);
    }
    
    #[test]
    fn test_save_and_load() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_adblock_config.json");
        
        // Créer et sauvegarder une configuration
        let mut config = AdBlockConfig::default();
        config.aggressive_mode = true;
        config.add_whitelist_domain("test.com".to_string());
        
        config.save_to_file(&config_path).unwrap();
        
        // Charger la configuration
        let loaded_config = AdBlockConfig::load_from_file(&config_path).unwrap();
        
        assert_eq!(loaded_config.aggressive_mode, true);
        assert!(loaded_config.is_whitelisted("test.com"));
        
        // Nettoyer
        fs::remove_file(config_path).ok();
    }
    
    #[test]
    fn test_load_or_default() {
        let temp_dir = std::env::temp_dir();
        let non_existent_path = temp_dir.join("non_existent_config.json");
        
        // Devrait créer une config par défaut
        let config = AdBlockConfig::load_or_default(&non_existent_path);
        assert!(config.enabled);
        
        // Nettoyer si le fichier a été créé
        fs::remove_file(non_existent_path).ok();
    }
}
