//! Sorgu geçmişi: `sorgular.jsonl` dosyasına satır satır eklenen kayıtlar.
//!
//! Her çalıştırılan sorgu için zaman damgası, veritabanı yolu, sorgu metni, süre, satır
//! sayısı ve hata durumu bir JSON satırı olarak eklenir. Dosya yalnızca **ekleme** kipinde
//! açılır; var olan satırlar değiştirilmez.
//!
//! Zaman damgası `std::time::SystemTime` ile alınır ve yalnızca **çıktı** olarak kullanılır;
//! hiçbir kararı etkilemez (WORKER_CONTRACT § 5.2).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hata::SahneHata;

/// Geçmiş dosyasının varsayılan adı.
pub const VARSAYILAN_DOSYA_ADI: &str = "sorgular.jsonl";

/// Tek bir geçmiş kaydı (bir JSON satırı).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GecmisKaydi {
    /// Unix saniye cinsinden zaman damgası (`SystemTime::now`).
    pub zaman: u64,
    /// Sorgunun çalıştırıldığı veritabanı dosyasının yolu.
    pub veritabani: String,
    /// Çalıştırılan sorgu metni.
    pub sorgu: String,
    /// Yürütme süresi (nanosecond); hata varsa 0.
    pub sure_ns: u128,
    /// Dönen satır sayısı; hata varsa 0.
    pub satir_sayisi: usize,
    /// Hata varsa hata metni, yoksa boş string.
    pub hata: String,
}

impl GecmisKaydi {
    /// Başarılı bir çalıştırma kaydı üretir.
    pub fn basarili(veritabani: &str, sorgu: &str, sure_ns: u128, satir_sayisi: usize) -> Self {
        Self {
            zaman: simdi_saniye(),
            veritabani: veritabani.to_string(),
            sorgu: sorgu.to_string(),
            sure_ns,
            satir_sayisi,
            hata: String::new(),
        }
    }

    /// Hatalı bir çalıştırma kaydı üretir.
    pub fn basarisiz(veritabani: &str, sorgu: &str, hata: &str) -> Self {
        Self {
            zaman: simdi_saniye(),
            veritabani: veritabani.to_string(),
            sorgu: sorgu.to_string(),
            sure_ns: 0,
            satir_sayisi: 0,
            hata: hata.to_string(),
        }
    }

    /// Kaydın JSON satırı metnini döndürür (satır sonu hariç).
    pub fn satir_metni(&self) -> Result<String, SahneHata> {
        serde_json::to_string(self).map_err(|hata| SahneHata::Json(hata.to_string()))
    }
}

/// Geçmiş dosyasının konumunu belirler.
///
/// Öncelik sırası: açıkça verilen yol → `SQLSTAGE_HOME` ortam değişkeni → yürütülebilirin
/// bulunduğu dizin. Böylece salt okunur bir USB'den çalıştırıldığında dosya program
/// dizinine yazılmaz.
pub fn gecis_dosyasi(acik: Option<&Path>) -> PathBuf {
    if let Some(yol) = acik {
        return yol.to_path_buf();
    }
    if let Some(cevher) = std::env::var_os("SQLSTAGE_HOME") {
        if !cevher.is_empty() {
            return PathBuf::from(cevher).join(VARSAYILAN_DOSYA_ADI);
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|yol| yol.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(VARSAYILAN_DOSYA_ADI)
}

/// Bir kaydı geçmiş dosyasına ekler (dosya yoksa oluşturulur).
///
/// # Hatalar
///
/// Dosya açılamazsa veya JSON üretilemezse hata döner. Yazma başarısız olursa geçmiş
/// kaydı atlanmaz; hata çağırana bildirilir.
pub fn kaydet(yol: &Path, kayit: &GecmisKaydi) -> Result<(), SahneHata> {
    let satir = kayit.satir_metni()?;
    if let Some(ust) = yol.parent() {
        if !ust.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(ust);
        }
    }
    let mut dosya = OpenOptions::new()
        .create(true)
        .append(true)
        .open(yol)
        .map_err(|kaynak| SahneHata::Io {
            yol: yol.display().to_string(),
            kaynak,
        })?;
    writeln!(dosya, "{satir}").map_err(|kaynak| SahneHata::Io {
        yol: yol.display().to_string(),
        kaynak,
    })
}

/// Geçmiş dosyasındaki tüm kayıtları okur.
///
/// # Hatalar
///
/// Dosya bulunamazsa hata döner; bozuk satırlar atlanır ve dosyaya dokunulmaz.
pub fn oku(yol: &Path) -> Result<Vec<GecmisKaydi>, SahneHata> {
    let icerik = std::fs::read_to_string(yol).map_err(|kaynak| SahneHata::Io {
        yol: yol.display().to_string(),
        kaynak,
    })?;
    let mut kayitlar = Vec::new();
    for satir in icerik.lines() {
        if satir.trim().is_empty() {
            continue;
        }
        // Bozuk satır geçmişi bozmamalıdır; atlanır.
        if let Ok(kayit) = serde_json::from_str::<GecmisKaydi>(satir) {
            kayitlar.push(kayit);
        }
    }
    Ok(kayitlar)
}

/// Geçmişteki 1 tabanlı numarayla bir sorguyu döndürür.
///
/// # Hatalar
///
/// Dosya okunamazsa veya numara geçerli bir kayıta denk gelmezse hata döner.
pub fn kayit_getir(yol: &Path, numara: usize) -> Result<GecmisKaydi, SahneHata> {
    if numara == 0 {
        return Err(SahneHata::SemaHatasi {
            mesaj: "geçmiş numarası 1'den başlar".to_string(),
        });
    }
    let kayitlar = oku(yol)?;
    let bulunan = kayitlar.into_iter().nth(numara - 1);
    bulunan.ok_or(SahneHata::SemaHatasi {
        mesaj: format!("geçmişte {numara} numaralı kayıt yok"),
    })
}

fn simdi_saniye() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |s| s.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gecici_yol(etiket: &str) -> PathBuf {
        let dizin =
            std::env::temp_dir().join(format!("sqlstage-gecmis-{}-{etiket}", std::process::id()));
        let _ = std::fs::create_dir_all(&dizin);
        dizin.join(VARSAYILAN_DOSYA_ADI)
    }

    #[test]
    fn kayit_eklenir_ve_okunur() {
        let yol = gecici_yol("ekle");
        let _ = std::fs::remove_file(&yol);
        let kayit = GecmisKaydi::basarili("a.db", "SELECT 1", 100, 3);
        if let Err(hata) = kaydet(&yol, &kayit) {
            panic!("beklenmeyen hata: {hata}");
        }
        let kayitlar = match oku(&yol) {
            Ok(k) => k,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(kayitlar.len(), 1);
        assert_eq!(kayitlar[0].sorgu, "SELECT 1");
        assert_eq!(kayitlar[0].satir_sayisi, 3);
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn birden_fazla_kayit_sirali_eklenir() {
        let yol = gecici_yol("coklu");
        let _ = std::fs::remove_file(&yol);
        for i in 1..=3 {
            let kayit = GecmisKaydi::basarili("a.db", &format!("SELECT {i}"), 1, i);
            if let Err(hata) = kaydet(&yol, &kayit) {
                panic!("beklenmeyen hata: {hata}");
            }
        }
        let kayitlar = match oku(&yol) {
            Ok(k) => k,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(kayitlar.len(), 3);
        assert_eq!(kayitlar[2].sorgu, "SELECT 3");
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn hatali_kayit_hata_metni_tasir() {
        let kayit = GecmisKaydi::basarisiz("a.db", "UPDATE t SET a=1", "yazma komutu reddedildi");
        assert!(!kayit.hata.is_empty());
        assert_eq!(kayit.satir_sayisi, 0);
    }

    #[test]
    fn numarayla_kayit_getirilir() {
        let yol = gecici_yol("numara");
        let _ = std::fs::remove_file(&yol);
        if let Err(hata) = kaydet(&yol, &GecmisKaydi::basarili("a.db", "bir", 1, 1)) {
            panic!("beklenmeyen hata: {hata}");
        }
        if let Err(hata) = kaydet(&yol, &GecmisKaydi::basarili("a.db", "iki", 1, 2)) {
            panic!("beklenmeyen hata: {hata}");
        }
        match kayit_getir(&yol, 2) {
            Ok(kayit) => assert_eq!(kayit.sorgu, "iki"),
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
        assert!(kayit_getir(&yol, 0).is_err());
        assert!(kayit_getir(&yol, 9).is_err());
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn bozuk_satirlar_atlanir() {
        let yol = gecici_yol("bozuk");
        let _ = std::fs::remove_file(&yol);
        if let Err(hata) = kaydet(&yol, &GecmisKaydi::basarili("a.db", "bir", 1, 1)) {
            panic!("beklenmeyen hata: {hata}");
        }
        if let Err(hata) = std::fs::OpenOptions::new()
            .append(true)
            .open(&yol)
            .and_then(|mut d| d.write_all(b"bu json degil\n"))
        {
            panic!("beklenmeyen hata: {hata}");
        }
        let kayitlar = match oku(&yol) {
            Ok(k) => k,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(kayitlar.len(), 1);
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn dosya_yoksa_hata_verir() {
        let yol = gecici_yol("yok");
        let _ = std::fs::remove_file(&yol);
        assert!(matches!(oku(&yol), Err(SahneHata::Io { .. })));
    }

    #[test]
    fn gecis_dosyasi_acik_yolu_tercih_eder() {
        let yol = gecis_dosyasi(Some(Path::new("C:/ozel/gecmis.jsonl")));
        assert_eq!(yol, PathBuf::from("C:/ozel/gecmis.jsonl"));
    }

    #[test]
    fn gecis_dosyasi_jsonl_adiyla_biter() {
        let yol = gecis_dosyasi(None);
        assert!(yol.ends_with(VARSAYILAN_DOSYA_ADI));
    }
}
