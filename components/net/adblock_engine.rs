// components/net/adblock_engine.rs
//
// Moteur de blocage de publicités pour Servo
// Basé sur adblock-rust de Brave

use adblock::Engine;
use adblock::lists::FilterSet;
use std::sync::Arc;
use std::path::Path;

/// Moteur de blocage de publicités
/// 
/// Utilise adblock-rust pour bloquer les publicités et trackers
/// au niveau des requêtes réseau.
pub struct AdBlockEngine {
    engine: Arc<Engine>,
    enabled: bool,
}

impl AdBlockEngine {
    /// Crée une nouvelle instance du moteur adblock
    pub fn new() -> Self {
        let engine = Engine::default();
        Self {
            engine: Arc::new(engine),
            enabled: true,
        }
    }
    
    /// Active ou désactive le blocage
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Vérifie si le blocage est activé
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Charge les listes de filtres depuis des fichiers
    /// 
    /// # Arguments
    /// * `filter_paths` - Chemins vers les fichiers de filtres
    /// 
    /// # Retourne
    /// Le nombre de règles chargées ou une erreur
    pub fn load_filters_from_files(&mut self, filter_paths: &[&Path]) -> Result<usize, String> {
        let mut filter_set = FilterSet::default();
        let mut total_rules = 0;
        
        for path in filter_paths {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    let rule_count = content.lines()
                        .filter(|line| !line.trim().is_empty() && !line.starts_with('!'))
                        .count();
                    
                    filter_set.add_filter_list(&content, Default::default());
                    total_rules += rule_count;
                    
                    println!("AdBlock: Chargé {} règles depuis {:?}", rule_count, path);
                }
                Err(e) => {
                    eprintln!("AdBlock: Erreur lors du chargement de {:?}: {}", path, e);
                    return Err(format!("Impossible de charger {:?}: {}", path, e));
                }
            }
        }
        
        let engine = Engine::from_filter_set(filter_set, true);
        self.engine = Arc::new(engine);
        
        println!("AdBlock: Total de {} règles chargées", total_rules);
        Ok(total_rules)
    }
    
    /// Charge des filtres depuis des strings
    /// 
    /// Utile pour les tests ou les filtres personnalisés
    pub fn load_filters_from_strings(&mut self, filter_lists: &[&str]) -> usize {
        let mut filter_set = FilterSet::default();
        let mut total_rules = 0;
        
        for list in filter_lists {
            let rule_count = list.lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('!'))
                .count();
            
            filter_set.add_filter_list(list, Default::default());
            total_rules += rule_count;
        }
        
        let engine = Engine::from_filter_set(filter_set, true);
        self.engine = Arc::new(engine);
        
        total_rules
    }
    
    /// Vérifie si une URL doit être bloquée
    /// 
    /// # Arguments
    /// * `url` - L'URL de la requête à vérifier
    /// * `source_url` - L'URL de la page qui fait la requête
    /// * `request_type` - Le type de ressource (script, image, stylesheet, etc.)
    /// 
    /// # Retourne
    /// `true` si la requête doit être bloquée, `false` sinon
    pub fn should_block(
        &self,
        url: &str,
        source_url: &str,
        request_type: &str,
    ) -> bool {
        // Si le blocage est désactivé, toujours autoriser
        if !self.enabled {
            return false;
        }
        
        // Créer la requête pour adblock-rust
        let request = match adblock::request::Request::new(url, source_url, request_type) {
            Ok(req) => req,
            Err(_) => {
                // En cas d'erreur de parsing, on ne bloque pas par défaut
                return false;
            }
        };
        
        // Vérifier contre les règles
        let result = self.engine.check_network_request(&request);
        
        // result.matched indique si une règle de blocage a été trouvée
        // result.exception indique si une règle d'exception (whitelist) existe
        result.matched && result.exception.is_none()
    }
    
    /// Version simplifiée pour bloquer uniquement par URL
    /// 
    /// Utilise le type "other" par défaut
    pub fn should_block_url(&self, url: &str) -> bool {
        self.should_block(url, "", "other")
    }
    
    /// Retourne une copie de l'Arc du moteur
    /// 
    /// Utile pour partager le moteur entre plusieurs threads
    pub fn clone_engine(&self) -> Arc<Engine> {
        Arc::clone(&self.engine)
    }
}

impl Default for AdBlockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AdBlockEngine {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            enabled: self.enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_engine_initialization() {
        let engine = AdBlockEngine::new();
        assert!(engine.is_enabled());
    }
    
    #[test]
    fn test_enable_disable() {
        let mut engine = AdBlockEngine::new();
        assert!(engine.is_enabled());
        
        engine.set_enabled(false);
        assert!(!engine.is_enabled());
        
        engine.set_enabled(true);
        assert!(engine.is_enabled());
    }
    
    #[test]
    fn test_basic_blocking() {
        let mut engine = AdBlockEngine::new();
        
        // Ajouter quelques règles de test
        let test_rules = r#"
||ads.example.com^
||tracker.com/pixel.gif
||analytics.example.net^$script
"#;
        engine.load_filters_from_strings(&[test_rules]);
        
        // Test de blocage
        assert!(engine.should_block_url("https://ads.example.com/banner.jpg"));
        assert!(engine.should_block_url("https://tracker.com/pixel.gif"));
        assert!(engine.should_block(
            "https://analytics.example.net/track.js",
            "https://mysite.com",
            "script"
        ));
        
        // Test de non-blocage
        assert!(!engine.should_block_url("https://example.com/content.jpg"));
        assert!(!engine.should_block_url("https://mysite.com/script.js"));
    }
    
    #[test]
    fn test_third_party_blocking() {
        let mut engine = AdBlockEngine::new();
        
        let test_rules = "||ads.example.com^$third-party\n";
        engine.load_filters_from_strings(&[test_rules]);
        
        // Bloqué en third-party context
        assert!(engine.should_block(
            "https://ads.example.com/ad.js",
            "https://mysite.com",
            "script"
        ));
        
        // Non bloqué en first-party context
        assert!(!engine.should_block(
            "https://ads.example.com/ad.js",
            "https://ads.example.com",
            "script"
        ));
    }
    
    #[test]
    fn test_resource_type_blocking() {
        let mut engine = AdBlockEngine::new();
        
        let test_rules = "||tracker.com^$image\n";
        engine.load_filters_from_strings(&[test_rules]);
        
        // Bloqué pour les images
        assert!(engine.should_block(
            "https://tracker.com/pixel.gif",
            "https://site.com",
            "image"
        ));
        
        // Non bloqué pour les scripts
        assert!(!engine.should_block(
            "https://tracker.com/analytics.js",
            "https://site.com",
            "script"
        ));
    }
    
    #[test]
    fn test_disabled_engine() {
        let mut engine = AdBlockEngine::new();
        
        let test_rules = "||ads.example.com^\n";
        engine.load_filters_from_strings(&[test_rules]);
        
        // Vérifie que le blocage fonctionne
        assert!(engine.should_block_url("https://ads.example.com/ad.js"));
        
        // Désactive le moteur
        engine.set_enabled(false);
        
        // Vérifie que rien n'est bloqué
        assert!(!engine.should_block_url("https://ads.example.com/ad.js"));
    }
    
    #[test]
    fn test_known_ad_domains() {
        let mut engine = AdBlockEngine::new();
        
        // Règles basées sur les vraies listes
        let common_ad_rules = r#"
||doubleclick.net^
||googlesyndication.com^
||googleadservices.com^
||facebook.com/tr/*
||connect.facebook.net/*/fbevents.js$script
||pagead2.googlesyndication.com^
"#;
        engine.load_filters_from_strings(&[common_ad_rules]);
        
        // Tests de domaines publicitaires connus
        assert!(engine.should_block_url("https://doubleclick.net/ad"));
        assert!(engine.should_block_url("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js"));
        assert!(engine.should_block_url("https://www.googleadservices.com/pagead/conversion.js"));
        
        // Sites normaux ne doivent pas être bloqués
        assert!(!engine.should_block_url("https://google.com"));
        assert!(!engine.should_block_url("https://facebook.com"));
    }
    
    #[test]
    fn test_whitelist_rules() {
        let mut engine = AdBlockEngine::new();
        
        let rules_with_exception = r#"
||ads.example.com^
@@||ads.example.com/whitelist/*
"#;
        engine.load_filters_from_strings(&[rules_with_exception]);
        
        // Doit être bloqué
        assert!(engine.should_block_url("https://ads.example.com/normal/ad.js"));
        
        // Doit être autorisé par la règle d'exception
        assert!(!engine.should_block_url("https://ads.example.com/whitelist/ad.js"));
    }
}
