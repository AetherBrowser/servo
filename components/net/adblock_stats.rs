// components/net/adblock_stats.rs
//
// Statistiques de blocage pour le moteur adblock

use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::RwLock;

/// Statistiques de blocage
/// 
/// Thread-safe via atomics et RwLock
pub struct AdBlockStats {
    /// Nombre total de requêtes bloquées
    pub blocked_requests: AtomicU64,
    
    /// Nombre total de requêtes autorisées
    pub allowed_requests: AtomicU64,
    
    /// Requêtes bloquées par domaine
    blocked_by_domain: RwLock<HashMap<String, u64>>,
    
    /// Requêtes bloquées par type
    blocked_by_type: RwLock<HashMap<String, u64>>,
}

impl AdBlockStats {
    /// Crée une nouvelle instance de statistiques
    pub fn new() -> Self {
        Self {
            blocked_requests: AtomicU64::new(0),
            allowed_requests: AtomicU64::new(0),
            blocked_by_domain: RwLock::new(HashMap::new()),
            blocked_by_type: RwLock::new(HashMap::new()),
        }
    }
    
    /// Incrémente le compteur de requêtes bloquées
    pub fn increment_blocked(&self) {
        self.blocked_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Incrémente le compteur de requêtes autorisées
    pub fn increment_allowed(&self) {
        self.allowed_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Enregistre une requête bloquée avec contexte
    /// 
    /// # Arguments
    /// * `url` - URL bloquée
    /// * `request_type` - Type de ressource (script, image, etc.)
    pub fn record_blocked(&self, url: &str, request_type: &str) {
        self.increment_blocked();
        
        // Extraire le domaine de l'URL
        if let Some(domain) = extract_domain(url) {
            let mut domains = self.blocked_by_domain.write().unwrap();
            *domains.entry(domain).or_insert(0) += 1;
        }
        
        // Enregistrer par type
        let mut types = self.blocked_by_type.write().unwrap();
        *types.entry(request_type.to_string()).or_insert(0) += 1;
    }
    
    /// Enregistre une requête autorisée
    pub fn record_allowed(&self, _url: &str, _request_type: &str) {
        self.increment_allowed();
    }
    
    /// Retourne les statistiques globales
    pub fn get_totals(&self) -> (u64, u64) {
        (
            self.blocked_requests.load(Ordering::Relaxed),
            self.allowed_requests.load(Ordering::Relaxed),
        )
    }
    
    /// Retourne le nombre de requêtes bloquées
    pub fn get_blocked_count(&self) -> u64 {
        self.blocked_requests.load(Ordering::Relaxed)
    }
    
    /// Retourne le nombre de requêtes autorisées
    pub fn get_allowed_count(&self) -> u64 {
        self.allowed_requests.load(Ordering::Relaxed)
    }
    
    /// Retourne le pourcentage de requêtes bloquées
    pub fn get_block_percentage(&self) -> f64 {
        let blocked = self.get_blocked_count() as f64;
        let total = (self.get_blocked_count() + self.get_allowed_count()) as f64;
        
        if total == 0.0 {
            0.0
        } else {
            (blocked / total) * 100.0
        }
    }
    
    /// Retourne les domaines les plus bloqués
    /// 
    /// # Arguments
    /// * `limit` - Nombre maximum de domaines à retourner
    pub fn get_top_blocked_domains(&self, limit: usize) -> Vec<(String, u64)> {
        let domains = self.blocked_by_domain.read().unwrap();
        
        let mut sorted: Vec<_> = domains.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        
        sorted
    }
    
    /// Retourne les types de ressources les plus bloqués
    pub fn get_blocked_by_type(&self) -> HashMap<String, u64> {
        self.blocked_by_type.read().unwrap().clone()
    }
    
    /// Réinitialise toutes les statistiques
    pub fn reset(&self) {
        self.blocked_requests.store(0, Ordering::Relaxed);
        self.allowed_requests.store(0, Ordering::Relaxed);
        
        self.blocked_by_domain.write().unwrap().clear();
        self.blocked_by_type.write().unwrap().clear();
    }
    
    /// Génère un rapport textuel des statistiques
    pub fn generate_report(&self) -> String {
        let (blocked, allowed) = self.get_totals();
        let percentage = self.get_block_percentage();
        let top_domains = self.get_top_blocked_domains(10);
        let types = self.get_blocked_by_type();
        
        let mut report = String::new();
        report.push_str("=== Statistiques AdBlock ===\n\n");
        report.push_str(&format!("Requêtes bloquées: {}\n", blocked));
        report.push_str(&format!("Requêtes autorisées: {}\n", allowed));
        report.push_str(&format!("Pourcentage bloqué: {:.2}%\n\n", percentage));
        
        if !top_domains.is_empty() {
            report.push_str("Top 10 des domaines bloqués:\n");
            for (i, (domain, count)) in top_domains.iter().enumerate() {
                report.push_str(&format!("  {}. {} - {} requêtes\n", i + 1, domain, count));
            }
            report.push_str("\n");
        }
        
        if !types.is_empty() {
            report.push_str("Blocages par type de ressource:\n");
            let mut sorted_types: Vec<_> = types.iter().collect();
            sorted_types.sort_by(|a, b| b.1.cmp(a.1));
            
            for (type_name, count) in sorted_types {
                report.push_str(&format!("  {}: {}\n", type_name, count));
            }
        }
        
        report
    }
}

impl Default for AdBlockStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Extrait le domaine d'une URL
fn extract_domain(url: &str) -> Option<String> {
    // Méthode simple d'extraction de domaine
    // Pour une implémentation plus robuste, utiliser une bibliothèque comme `url`
    
    let url = url.trim();
    
    // Retirer le protocole
    let without_protocol = if let Some(idx) = url.find("://") {
        &url[idx + 3..]
    } else {
        url
    };
    
    // Retirer le chemin
    let domain = if let Some(idx) = without_protocol.find('/') {
        &without_protocol[..idx]
    } else {
        without_protocol
    };
    
    // Retirer le port
    let domain = if let Some(idx) = domain.find(':') {
        &domain[..idx]
    } else {
        domain
    };
    
    if domain.is_empty() {
        None
    } else {
        Some(domain.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stats_initialization() {
        let stats = AdBlockStats::new();
        assert_eq!(stats.get_blocked_count(), 0);
        assert_eq!(stats.get_allowed_count(), 0);
        assert_eq!(stats.get_block_percentage(), 0.0);
    }
    
    #[test]
    fn test_increment_counters() {
        let stats = AdBlockStats::new();
        
        stats.increment_blocked();
        stats.increment_blocked();
        stats.increment_allowed();
        
        assert_eq!(stats.get_blocked_count(), 2);
        assert_eq!(stats.get_allowed_count(), 1);
    }
    
    #[test]
    fn test_record_blocked() {
        let stats = AdBlockStats::new();
        
        stats.record_blocked("https://ads.example.com/banner.js", "script");
        stats.record_blocked("https://ads.example.com/pixel.gif", "image");
        stats.record_blocked("https://tracker.com/analytics.js", "script");
        
        assert_eq!(stats.get_blocked_count(), 3);
        
        let top_domains = stats.get_top_blocked_domains(10);
        assert_eq!(top_domains.len(), 2);
        assert_eq!(top_domains[0].0, "ads.example.com");
        assert_eq!(top_domains[0].1, 2);
        
        let types = stats.get_blocked_by_type();
        assert_eq!(types.get("script"), Some(&2));
        assert_eq!(types.get("image"), Some(&1));
    }
    
    #[test]
    fn test_block_percentage() {
        let stats = AdBlockStats::new();
        
        stats.increment_blocked();
        stats.increment_blocked();
        stats.increment_blocked();
        stats.increment_allowed();
        
        // 3 bloquées sur 4 total = 75%
        assert_eq!(stats.get_block_percentage(), 75.0);
    }
    
    #[test]
    fn test_reset() {
        let stats = AdBlockStats::new();
        
        stats.record_blocked("https://ads.example.com/ad.js", "script");
        stats.increment_allowed();
        
        assert_eq!(stats.get_blocked_count(), 1);
        assert_eq!(stats.get_allowed_count(), 1);
        
        stats.reset();
        
        assert_eq!(stats.get_blocked_count(), 0);
        assert_eq!(stats.get_allowed_count(), 0);
        assert!(stats.get_top_blocked_domains(10).is_empty());
    }
    
    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/path"), Some("example.com".to_string()));
        assert_eq!(extract_domain("http://sub.example.com:8080/path"), Some("sub.example.com".to_string()));
        assert_eq!(extract_domain("example.com/path"), Some("example.com".to_string()));
        assert_eq!(extract_domain("https://example.com"), Some("example.com".to_string()));
        assert_eq!(extract_domain(""), None);
    }
    
    #[test]
    fn test_generate_report() {
        let stats = AdBlockStats::new();
        
        stats.record_blocked("https://ads.example.com/ad.js", "script");
        stats.record_blocked("https://tracker.com/pixel.gif", "image");
        stats.increment_allowed();
        
        let report = stats.generate_report();
        
        assert!(report.contains("Requêtes bloquées: 2"));
        assert!(report.contains("Requêtes autorisées: 1"));
        assert!(report.contains("ads.example.com"));
        assert!(report.contains("tracker.com"));
    }
}
