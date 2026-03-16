// components/net/adblock_updater.rs
//
// Service de mise à jour automatique des listes de filtres

use crate::adblock_config::AdBlockConfig;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Service de mise à jour des listes de filtres
pub struct FilterUpdater {
    config: AdBlockConfig,
    last_update: SystemTime,
}

impl FilterUpdater {
    /// Crée un nouveau service de mise à jour
    pub fn new(config: AdBlockConfig) -> Self {
        Self {
            config,
            last_update: SystemTime::UNIX_EPOCH,
        }
    }
    
    /// Vérifie si les filtres doivent être mis à jour
    /// 
    /// Compare le temps écoulé depuis la dernière mise à jour
    /// avec l'intervalle configuré.
    pub fn should_update(&self) -> bool {
        if !self.config.auto_update {
            return false;
        }
        
        let update_interval = Duration::from_secs(self.config.update_interval_hours * 60 * 60);
        
        match SystemTime::now().duration_since(self.last_update) {
            Ok(duration) => duration > update_interval,
            Err(_) => true, // En cas d'erreur, on considère qu'il faut mettre à jour
        }
    }
    
    /// Met à jour tous les filtres activés
    /// 
    /// Télécharge les listes de filtres depuis les URLs configurées
    /// et les sauvegarde dans les fichiers locaux.
    pub async fn update_filters(&mut self) -> Result<UpdateReport, UpdateError> {
        if !self.should_update() {
            return Ok(UpdateReport {
                success_count: 0,
                failed_count: 0,
                skipped: true,
                errors: vec![],
            });
        }
        
        println!("AdBlock: Mise à jour des filtres...");
        
        let mut report = UpdateReport::default();
        
        // Récupérer les URLs activées
        let enabled_urls = self.config.get_enabled_filter_urls();
        
        for (i, url) in enabled_urls.iter().enumerate() {
            // Déterminer le chemin de sauvegarde
            let path = if i < self.config.filter_lists.len() {
                self.config.filter_lists[i].clone()
            } else {
                // Pour les listes personnalisées, générer un nom de fichier
                let filename = format!("custom_{}.txt", i);
                PathBuf::from("resources/adblock").join(filename)
            };
            
            match self.download_filter(url, &path).await {
                Ok(_) => {
                    println!("AdBlock: ✓ Mis à jour: {:?}", path);
                    report.success_count += 1;
                }
                Err(e) => {
                    eprintln!("AdBlock: ✗ Échec: {:?} - {}", path, e);
                    report.failed_count += 1;
                    report.errors.push(format!("{:?}: {}", path, e));
                }
            }
        }
        
        self.last_update = SystemTime::now();
        
        println!("AdBlock: Mise à jour terminée - {} réussies, {} échouées", 
                 report.success_count, report.failed_count);
        
        Ok(report)
    }
    
    /// Télécharge une liste de filtres depuis une URL
    async fn download_filter(&self, url: &str, path: &PathBuf) -> Result<(), UpdateError> {
        // Télécharger le contenu
        let response = reqwest::get(url)
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(UpdateError::HttpError(response.status().as_u16()));
        }
        
        let content = response
            .text()
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;
        
        // Valider le contenu (vérifier qu'il s'agit bien d'une liste de filtres)
        if !self.is_valid_filter_list(&content) {
            return Err(UpdateError::InvalidFilterList);
        }
        
        // Créer le dossier parent si nécessaire
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| UpdateError::IoError(e.to_string()))?;
        }
        
        // Sauvegarder le fichier
        std::fs::write(path, content)
            .map_err(|e| UpdateError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Valide qu'un contenu est une liste de filtres valide
    fn is_valid_filter_list(&self, content: &str) -> bool {
        // Vérifications basiques
        if content.is_empty() {
            return false;
        }
        
        // Une liste de filtres valide devrait contenir au moins quelques règles
        let rule_count = content.lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && 
                !trimmed.starts_with('!') && 
                !trimmed.starts_with('#')
            })
            .count();
        
        // Au moins 10 règles pour être considéré valide
        rule_count >= 10
    }
    
    /// Force une mise à jour immédiate
    pub async fn force_update(&mut self) -> Result<UpdateReport, UpdateError> {
        self.last_update = SystemTime::UNIX_EPOCH;
        self.update_filters().await
    }
    
    /// Retourne le temps depuis la dernière mise à jour
    pub fn time_since_last_update(&self) -> Option<Duration> {
        SystemTime::now().duration_since(self.last_update).ok()
    }
}

/// Rapport de mise à jour
#[derive(Debug, Default)]
pub struct UpdateReport {
    pub success_count: usize,
    pub failed_count: usize,
    pub skipped: bool,
    pub errors: Vec<String>,
}

impl UpdateReport {
    pub fn is_success(&self) -> bool {
        self.failed_count == 0 && !self.skipped
    }
}

/// Erreurs de mise à jour
#[derive(Debug)]
pub enum UpdateError {
    NetworkError(String),
    HttpError(u16),
    IoError(String),
    InvalidFilterList,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NetworkError(e) => write!(f, "Erreur réseau: {}", e),
            UpdateError::HttpError(code) => write!(f, "Erreur HTTP {}", code),
            UpdateError::IoError(e) => write!(f, "Erreur I/O: {}", e),
            UpdateError::InvalidFilterList => write!(f, "Liste de filtres invalide"),
        }
    }
}

impl std::error::Error for UpdateError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_should_update() {
        let config = AdBlockConfig::default();
        let updater = FilterUpdater::new(config);
        
        // Devrait vouloir mettre à jour car last_update est UNIX_EPOCH
        assert!(updater.should_update());
    }
    
    #[test]
    fn test_should_not_update_when_disabled() {
        let mut config = AdBlockConfig::default();
        config.auto_update = false;
        
        let updater = FilterUpdater::new(config);
        assert!(!updater.should_update());
    }
    
    #[test]
    fn test_is_valid_filter_list() {
        let config = AdBlockConfig::default();
        let updater = FilterUpdater::new(config);
        
        // Liste valide
        let valid_list = r#"
! Title: Test Filter List
! Description: A test list
||ads.example.com^
||tracker.com^
||analytics.net^
/banner/*
/tracking/*
/ads.js
||google-analytics.com^
||doubleclick.net^
||facebook.com/tr/*
||pagead2.googlesyndication.com^
"#;
        assert!(updater.is_valid_filter_list(valid_list));
        
        // Liste invalide (trop courte)
        let invalid_list = "||ads.com^\n";
        assert!(!updater.is_valid_filter_list(invalid_list));
        
        // Liste vide
        assert!(!updater.is_valid_filter_list(""));
    }
    
    #[test]
    fn test_time_since_last_update() {
        let config = AdBlockConfig::default();
        let updater = FilterUpdater::new(config);
        
        let duration = updater.time_since_last_update().unwrap();
        
        // Devrait être très long car last_update est UNIX_EPOCH
        assert!(duration.as_secs() > 1_000_000_000);
    }
}
