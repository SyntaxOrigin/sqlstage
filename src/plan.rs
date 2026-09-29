//! Sorgu plan açıklaması ve tarama maliyeti tahmini.
//!
//! SQLStage bir sorgu iyileştiricisi değildir; bu modül gerçek bir `EXPLAIN QUERY PLAN`
//! üretmez. Bunun yerine sorgunun metnini, okunacak tabloyu, kaç b-tree sayfasının
//! dolaşılacağını ve okunması beklenen bayt sayısını **yapısal sayma** ile tahmin eder.
//!
//! Tahmin gerçek bir sorgu çalıştırılmadan üretildiği için her çıktı "ölçülmedi" uyarısı
//! taşır; bu, raporun b01'de öne çıkardığı öğretici amacın dürüst karşılığıdır.

use crate::hata::SahneHata;
use crate::sema::sema_oku;
use crate::sql::ast::Sorgu;
use crate::sql::yurutucu::tablo_bul;
use crate::veritabani::Veritabani;

/// Tahminin dayandığı ölçümler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Sorgunun özgün metni.
    pub sorgu: String,
    /// Okunacak tablo.
    pub tablo: String,
    /// Tablonun kök sayfa numarası.
    pub kok_sayfa: u32,
    /// Dolaşılacak b-tree sayfası sayısı (yapısal sayma ile **ölçülmüş**).
    pub sayfa: u32,
    /// Sayfa boyutu (bayt).
    pub sayfa_boyutu: u32,
    /// Yaprak hücrelerinin toplam bildirilen yükü (bayt).
    pub yuk_toplami: u64,
    /// Filtredeki karşılaştırma sayısı.
    pub karsilastirma: usize,
    /// `ORDER BY` gerektiriyorsa `true` (tam satır sıralaması gerekir).
    pub siralama: bool,
    /// `LIMIT` uygulanıyorsa `true` (satırlar yine de taranır, yalnızca yazdırılmaz).
    pub dilimleme: bool,
}

impl Plan {
    /// Tahmin edilen okuma maliyetini bayt olarak döndürür.
    pub fn tahmini_bayt(&self) -> u64 {
        u64::from(self.sayfa) * u64::from(self.sayfa_boyutu)
    }

    /// Planı okunabilir çok satırlı metne çevirir.
    pub fn metin(&self) -> String {
        let mut cikti = String::new();
        cikti.push_str("Sorgu planı (tahmin; sorgu çalıştırılmadan ÖLÇÜLMEDİ)\n");
        cikti.push_str(&format!("  sorgu          : {}\n", self.sorgu));
        cikti.push_str("  erişim         : tam tablo taraması (TABLO TARAMA)\n");
        cikti.push_str(&format!("  tablo          : {}\n", self.tablo));
        cikti.push_str(&format!("  kök sayfa      : {}\n", self.kok_sayfa));
        cikti.push_str(&format!("  sayfa boyutu   : {} bayt\n", self.sayfa_boyutu));
        cikti.push_str(&format!(
            "  dolaşılan sayfa: {} (yapısal sayma ile ölçüldü)\n",
            self.sayfa
        ));
        cikti.push_str(&format!(
            "  okuma tahmini  : ~{} bayt ({} KiB)\n",
            self.tahmini_bayt(),
            self.tahmini_bayt() / 1024
        ));
        cikti.push_str(&format!("  hücre yükü     : ~{} bayt\n", self.yuk_toplami));
        cikti.push_str(&format!(
            "  filtre         : {} karşılaştırma (indeks kullanılmaz)\n",
            self.karsilastirma
        ));
        cikti.push_str(&format!(
            "  sıralama       : {}\n",
            if self.siralama {
                "tam satır sıralaması gerekir"
            } else {
                "yok"
            }
        ));
        cikti.push_str(&format!(
            "  dilimleme      : {}\n",
            if self.dilimleme {
                "LIMIT/OFFSET sonuca uygulanır; taranan satır sayısı azalmaz"
            } else {
                "yok"
            }
        ));
        cikti.push_str(
            "  uyarı          : bu bir sorgu iyileştiricisi değildir; SQLite'in \
             EXPLAIN QUERY PLAN çıktısıyla karşılaştırılamaz.\n",
        );
        cikti
    }
}

/// Bir sorgu için plan tahmini üretir.
///
/// # Hatalar
///
/// Tablo bulunamazsa veya tablo ağacı bozuksa hata döner.
pub fn plan_uret(db: &Veritabani, sorgu: &Sorgu) -> Result<Plan, SahneHata> {
    let sema = sema_oku(db)?;
    let tablo = tablo_bul(&sema, &sorgu.tablo)?;
    let (_, _, yuk_toplami) = db.tablo_yapisini_gez(tablo.kok_sayfa)?;
    Ok(Plan {
        sorgu: sorgu.metin.clone(),
        tablo: tablo.ad.clone(),
        kok_sayfa: tablo.kok_sayfa,
        sayfa: tablo.taranan_sayfa,
        sayfa_boyutu: db.baslik().sayfa_boyutu,
        yuk_toplami,
        karsilastirma: sorgu
            .filtre
            .as_ref()
            .map_or(0, |f| f.karsilastirma_sayisi()),
        siralama: !sorgu.siralama.is_empty(),
        dilimleme: sorgu.limit.is_some() || sorgu.ofset.is_some(),
    })
}

/// Bir sorgu için plan açıklaması üretip metne çevirir.
pub fn plan_metni(db: &Veritabani, sorgu: &Sorgu) -> Result<String, SahneHata> {
    Ok(plan_uret(db, sorgu)?.metin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deger::Deger;
    use crate::sql::parser::ayikla;
    use crate::veritabani::Veritabani;
    use crate::yazici::{veritabani_goruntusu_sayfa_boyutu, TabloYazimi, TEST_SAYFA_BOYUTU};

    fn ornek_db(etiket: &str) -> (std::path::PathBuf, Veritabani) {
        let mut satirlar = Vec::new();
        for i in 1..=100i64 {
            satirlar.push((i, vec![Deger::Tam(i), Deger::Metin(format!("k{i}"))]));
        }
        let goruntu = match veritabani_goruntusu_sayfa_boyutu(
            &[TabloYazimi {
                ad: "kisi".into(),
                sql: "CREATE TABLE kisi(id INTEGER PRIMARY KEY, ad TEXT)".into(),
                satirlar,
            }],
            TEST_SAYFA_BOYUTU,
        ) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let dizin =
            std::env::temp_dir().join(format!("sqlstage-plan-{}-{etiket}", std::process::id()));
        let _ = std::fs::create_dir_all(&dizin);
        let yol = dizin.join("p.db");
        std::fs::write(&yol, goruntu).unwrap_or_default();
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        (dizin, db)
    }

    #[test]
    fn plan_maliyet_tahmini_uretir() {
        let (dizin, db) = ornek_db("maliyet");
        let sorgu = match ayikla("SELECT * FROM kisi WHERE ad = 'k1'") {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let plan = match plan_uret(&db, &sorgu) {
            Ok(p) => p,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(plan.tablo, "kisi");
        assert!(plan.sayfa > 0);
        assert_eq!(plan.karsilastirma, 1);
        assert!(!plan.siralama);
        assert!(!plan.dilimleme);
        assert_eq!(plan.tahmini_bayt(), u64::from(plan.sayfa) * 512);
        let _ = std::fs::remove_dir_all(&dizin);
    }

    #[test]
    fn plan_metni_olculmedi_uyarisi_icerir() {
        let (dizin, db) = ornek_db("uyari");
        let sorgu = match ayikla("SELECT id FROM kisi ORDER BY ad DESC LIMIT 5") {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let metin = match plan_metni(&db, &sorgu) {
            Ok(m) => m,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(metin.contains("ÖLÇÜLMEDİ"), "{metin}");
        assert!(metin.contains("kisi"));
        assert!(metin.contains("tam satır sıralaması"));
        assert!(metin.contains("LIMIT/OFFSET"));
        assert!(metin.contains("EXPLAIN QUERY PLAN"));
        let _ = std::fs::remove_dir_all(&dizin);
    }

    #[test]
    fn bilinmeyen_tablo_planda_hata_verir() {
        let (dizin, db) = ornek_db("bilinmeyen");
        let sorgu = match ayikla("SELECT * FROM yok") {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(plan_uret(&db, &sorgu).is_err());
        let _ = std::fs::remove_dir_all(&dizin);
    }
}
