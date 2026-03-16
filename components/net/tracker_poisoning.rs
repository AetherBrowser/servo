// components/net/tracker_poisoning.rs
//
// Module de "tracker poisoning" - Renvoie de fausses informations aux trackers
// au lieu de les bloquer complètement

use rand::Rng;
use rand::distributions::Alphanumeric;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Configuration pour le tracker poisoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerPoisoningConfig {
    pub enabled: bool,
    pub randomize_user_agent: bool,
    pub randomize_fingerprint: bool,
    pub fake_geolocation: bool,
    pub fake_tracking_cookies: bool,
    pub fake_screen_resolution: bool,
    pub rotation_interval_minutes: u64,
}

impl Default for TrackerPoisoningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            randomize_user_agent: true,
            randomize_fingerprint: true,
            fake_geolocation: true,
            fake_tracking_cookies: true,
            fake_screen_resolution: true,
            rotation_interval_minutes: 30,
        }
    }
}

/// Générateur de fausses données
pub struct FakeDataGenerator {
    pub config: TrackerPoisoningConfig,
}

impl FakeDataGenerator {
    pub fn new(config: TrackerPoisoningConfig) -> Self {
        Self { config }
    }
    
    pub fn generate_fake_user_agent(&self) -> String {
        use rand::seq::SliceRandom;
        let user_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ];
        
        user_agents.choose(&mut rand::thread_rng()).unwrap().to_string()
    }
    
    pub fn generate_fake_location(&self) -> (f64, f64) {
        use rand::seq::SliceRandom;
        let locations = vec![
            (48.8566, 2.3522), (51.5074, -0.1278), (40.7128, -74.0060),
            (35.6762, 139.6503), (37.7749, -122.4194),
        ];
        
        let mut rng = rand::thread_rng();
        let base = locations.choose(&mut rng).unwrap();
        let noise_lat = (rng.gen_range(0.0..1.0) - 0.5) * 0.1;
        let noise_lon = (rng.gen_range(0.0..1.0) - 0.5) * 0.1;
        
        (base.0 + noise_lat, base.1 + noise_lon)
    }
    
    pub fn generate_fake_screen_resolution(&self) -> (u32, u32) {
        use rand::seq::SliceRandom;
        let resolutions = vec![
            (1920, 1080), (2560, 1440), (1366, 768), (1280, 720),
        ];
        
        *resolutions.choose(&mut rand::thread_rng()).unwrap()
    }
    
    pub fn generate_fake_canvas_hash(&self) -> String {
        let mut rng = rand::thread_rng();
        (0..32).map(|_| {
            let idx = rng.gen_range(0..16);
            std::char::from_digit(idx, 16).unwrap()
        }).collect()
    }
    
    pub fn generate_fake_webgl_hash(&self) -> String {
        use rand::seq::SliceRandom;
        let vendors = vec!["Google Inc. (NVIDIA)", "Google Inc. (Intel)", "Apple Inc."];
        let renderers = vec!["ANGLE (NVIDIA)", "ANGLE (Intel)", "Apple M1"];
        
        let mut rng = rand::thread_rng();
        format!("{}|{}", 
            vendors.choose(&mut rng).unwrap(),
            renderers.choose(&mut rng).unwrap()
        )
    }
    
    pub fn generate_fake_battery_info(&self) -> (f64, bool, f64) {
        let mut rng = rand::thread_rng();
        let level = rng.gen_range(0.2..1.0);
        let charging = rng.gen_range(0..2) == 1;
        let time = if charging { rng.gen_range(600.0..7200.0) } else { f64::INFINITY };
        (level, charging, time)
    }
    
    pub fn generate_fake_timezone_offset(&self) -> i32 {
        use rand::seq::SliceRandom;
        let offsets = vec![-480, -300, 0, 60, 120, 540];
        *offsets.choose(&mut rand::thread_rng()).unwrap()
    }
    
    pub fn generate_fake_languages(&self) -> Vec<String> {
        use rand::seq::SliceRandom;
        let lang_sets = vec![
            vec!["en-US", "en"],
            vec!["fr-FR", "fr", "en"],
            vec!["de-DE", "de", "en"],
        ];
        
        lang_sets.choose(&mut rand::thread_rng())
            .unwrap()
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }
    
    pub fn generate_fake_tracking_id(&self) -> String {
        let mut rng = rand::thread_rng();
        let timestamp = rng.gen_range(1000000000..2000000000);
        let random: String = (0..10)
            .map(|_| rng.sample(Alphanumeric))
            .map(char::from)
            .collect();
        format!("GA1.2.{}.{}", timestamp, random)
    }
    
    pub fn generate_fake_device_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
    
    pub fn generate_fake_fonts(&self) -> Vec<String> {
        vec!["Arial", "Helvetica", "Times New Roman", "Verdana"]
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }
    
    pub fn generate_fake_plugins(&self) -> Vec<String> {
        use rand::seq::SliceRandom;
        let sets = vec![
            vec!["Chrome PDF Plugin"],
            vec!["PDF Viewer"],
            vec![],
        ];
        
        sets.choose(&mut rand::thread_rng())
            .unwrap()
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }
    
    pub fn generate_fake_profile(&self) -> FakeProfile {
        FakeProfile {
            user_agent: self.generate_fake_user_agent(),
            location: self.generate_fake_location(),
            screen_resolution: self.generate_fake_screen_resolution(),
            canvas_hash: self.generate_fake_canvas_hash(),
            webgl_hash: self.generate_fake_webgl_hash(),
            battery_info: self.generate_fake_battery_info(),
            timezone_offset: self.generate_fake_timezone_offset(),
            languages: self.generate_fake_languages(),
            tracking_id: self.generate_fake_tracking_id(),
            device_id: self.generate_fake_device_id(),
            fonts: self.generate_fake_fonts(),
            plugins: self.generate_fake_plugins(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FakeProfile {
    pub user_agent: String,
    pub location: (f64, f64),
    pub screen_resolution: (u32, u32),
    pub canvas_hash: String,
    pub webgl_hash: String,
    pub battery_info: (f64, bool, f64),
    pub timezone_offset: i32,
    pub languages: Vec<String>,
    pub tracking_id: String,
    pub device_id: String,
    pub fonts: Vec<String>,
    pub plugins: Vec<String>,
}

pub struct TrackerPoisoner {
    pub generator: FakeDataGenerator,
    current_profile: Mutex<FakeProfile>,
    last_rotation: Mutex<std::time::SystemTime>,
}

impl TrackerPoisoner {
    pub fn new(config: TrackerPoisoningConfig) -> Self {
        let generator = FakeDataGenerator::new(config.clone());
        let current_profile = generator.generate_fake_profile();
        
        Self {
            generator,
            current_profile: Mutex::new(current_profile),
            last_rotation: Mutex::new(std::time::SystemTime::now()),
        }
    }
    
    fn should_rotate(&self) -> bool {
        let last = self.last_rotation.lock().unwrap();
        let elapsed = last.elapsed().unwrap_or_default();
        let duration = std::time::Duration::from_secs(self.generator.config.rotation_interval_minutes * 60);
        elapsed >= duration
    }
    
    pub fn rotate_profile(&self) {
        if self.should_rotate() {
            println!("TrackerPoisoning: Rotation du profil");
            let new_profile = self.generator.generate_fake_profile();
            *self.current_profile.lock().unwrap() = new_profile;
            *self.last_rotation.lock().unwrap() = std::time::SystemTime::now();
        }
    }
    
    pub fn get_current_profile(&self) -> FakeProfile {
        self.rotate_profile();
        self.current_profile.lock().unwrap().clone()
    }
    
    pub fn poison_request_headers(&self, headers: &mut http::HeaderMap) {
        let profile = self.get_current_profile();
        
        if self.generator.config.randomize_user_agent {
            if let Ok(value) = http::HeaderValue::from_str(&profile.user_agent) {
                headers.insert(http::header::USER_AGENT, value);
            }
        }
    }
    
    pub fn generate_fake_tracker_response(&self, _request_url: &str) -> Vec<u8> {
        vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00,
            0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
            0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x2C, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44,
            0x01, 0x00, 0x3B,
        ]
    }
}
