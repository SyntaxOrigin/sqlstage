//! Terminal ASCII şema diyagramı.
//!
//! `tiny-skia` yerine kutu ve ok karakterleriyle çalışan bir metin üreticisi kullanılır.
//! Yerleşim kutusu-ok grafiğinden çok "tablo kartları + ilişki satırları" biçimindedir;
//! bu, uzun kolon adlarında okunur kalmayı korur ve çizim algoritması gerektirmez.
//!
//! Kaynak: fikir raporu b05 "Şema diyagramı" maddesi; `tiny-skia` yerine ASCII karşılığı.

use crate::sema::Sema;

/// Üretilen diyagram metninde kullanılan kutu genişliği sınırı.
const KOLON_ENI: usize = 24;

/// Bir tablonun ASCII kutusu ile çizilmiş kartı.
pub struct Kart {
    /// Tablo adı.
    pub ad: String,
    /// Satır biçimindeki kolon satırları (`id INTEGER PK` gibi).
    pub satirlar: Vec<String>,
    /// Kutu üst/alt kenarı.
    kenar: String,
    /// Sol kenar.
    kenar_sol: String,
    /// Sağ kenar.
    kenar_sag: String,
}

impl Kart {
    /// Kutuyu metne çevirir.
    pub fn metin(&self) -> String {
        let mut cikti = String::new();
        cikti.push_str(&format!("{}{}\n", self.kenar, self.kenar));
        cikti.push_str(&format!(
            "{}{}{}\n",
            self.kenar_sol,
            ortala(&self.ad, self.kenar.len().saturating_sub(2)),
            self.kenar_sag
        ));
        cikti.push_str(&format!("{}{}\n", self.kenar, self.kenar));
        for satir in &self.satirlar {
            cikti.push_str(&format!(
                "{} {}{} {}\n",
                self.kenar_sol,
                satir,
                " ".repeat(self.genislik().saturating_sub(satir.chars().count() + 1)),
                self.kenar_sag
            ));
        }
        cikti.push_str(&format!("{}{}", self.kenar, self.kenar));
        cikti
    }

    fn genislik(&self) -> usize {
        let en = self
            .satirlar
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0)
            .max(self.ad.chars().count());
        en + 4
    }
}

fn ortala(metin: &str, en: usize) -> String {
    let uzunluk = metin.chars().count();
    if uzunluk >= en {
        return metin.to_string();
    }
    let sol = (en - uzunluk) / 2;
    let sag = en - uzunluk - sol;
    format!("{}{}{}", " ".repeat(sol), metin, " ".repeat(sag))
}

/// Bir tablo için ASCII kartı üretir.
pub fn kart_uret(tablo: &crate::sema::Tablo) -> Kart {
    let satirlar: Vec<String> = tablo
        .kolonlar
        .iter()
        .map(|k| {
            let mut parcalar = vec![k.ad.clone(), k.tur.ad().to_string()];
            if k.birincil_anahtar {
                parcalar.push("PK".to_string());
            }
            if k.not_null {
                parcalar.push("NN".to_string());
            }
            let satir = parcalar.join(" ");
            crate::izgara::kisalt(&satir, KOLON_ENI)
        })
        .collect();
    let genislik = satirlar
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(tablo.ad.chars().count())
        + 4;
    let kenar = "+".to_string() + &"-".repeat(genislik);
    Kart {
        ad: tablo.ad.clone(),
        satirlar,
        kenar,
        kenar_sol: "|".to_string(),
        kenar_sag: "|".to_string(),
    }
}

/// Şemadan tam ASCII diyagram metni üretir.
///
/// Diyagram iki bölümden oluşur: tablo kartları ve yabancı anahtar ilişkileri. Şemada
/// tablo yoksa açıklayıcı bir satır döner.
pub fn diyagram_uret(sema: &Sema) -> String {
    if sema.tablolar.is_empty() {
        return "(şemada tablo yok)".to_string();
    }
    let mut cikti = String::new();
    cikti.push_str("== Tablolar ==\n");
    for tablo in &sema.tablolar {
        cikti.push_str(&kart_uret(tablo).metin());
        cikti.push('\n');
    }

    let iliskiler = iliski_satirlari(sema);
    if !iliskiler.is_empty() {
        cikti.push_str("\n== Iliskiler ==\n");
        for satir in iliskiler {
            cikti.push_str(&satir);
            cikti.push('\n');
        }
    }
    cikti
}

/// Yabancı anahtar ilişkilerini `kaynak 1 --- N hedef (kolon > kolon)` biçiminde üretir.
pub fn iliski_satirlari(sema: &Sema) -> Vec<String> {
    let mut satirlar = Vec::new();
    for tablo in &sema.tablolar {
        for yabanci in &tablo.yabancilar {
            let kaynak = yabanci.kolonlar.join(", ");
            let hedef = if yabanci.hedef_kolonlar.is_empty() {
                String::new()
            } else {
                format!(" ({})", yabanci.hedef_kolonlar.join(", "))
            };
            satirlar.push(format!(
                "{} [1] --- [N] {}{}  ({})",
                yabanci.hedef_tablo, tablo.ad, hedef, kaynak
            ));
        }
    }
    satirlar
}

/// Şemayı tek satırlık özet olarak üretir (tablo, kolon sayısı, satır sayısı).
pub fn ozet_satirlari(sema: &Sema) -> Vec<String> {
    sema.tablolar
        .iter()
        .map(|t| {
            format!(
                "{:<24} {:>3} kolon  {:>6} satir  kok sayfa {}",
                t.ad,
                t.kolonlar.len(),
                t.satir_sayisi,
                t.kok_sayfa
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::{Kolon, KolonTuru, Tablo, YabanciAnahtar};

    fn ornek_tablo() -> Tablo {
        Tablo {
            ad: "musteriler".into(),
            kok_sayfa: 2,
            sql: String::new(),
            kolonlar: vec![
                Kolon {
                    ad: "id".into(),
                    tur: KolonTuru::Tamsayi,
                    not_null: true,
                    birincil_anahtar: true,
                    varsayilan: None,
                },
                Kolon {
                    ad: "ad".into(),
                    tur: KolonTuru::Metin,
                    not_null: false,
                    birincil_anahtar: false,
                    varsayilan: None,
                },
            ],
            yabancilar: Vec::new(),
            gizli: false,
            satir_sayisi: 2,
            taranan_sayfa: 1,
            rowid_alan: Some(0),
        }
    }

    #[test]
    fn bos_sema_mesaj_dondurur() {
        let sema = Sema::default();
        assert_eq!(diyagram_uret(&sema), "(şemada tablo yok)");
    }

    #[test]
    fn kart_cerceveleri_ayni_boyda() {
        let metin = kart_uret(&ornek_tablo()).metin();
        let satirlar: Vec<&str> = metin.lines().filter(|l| l.starts_with('+')).collect();
        assert_eq!(satirlar.len(), 3, "üst, başlık altı ve alt kenar");
        assert_eq!(satirlar[0].len(), satirlar[1].len());
        assert_eq!(satirlar[1].len(), satirlar[2].len());
    }

    #[test]
    fn kart_icerigi_kolonlari_gosterir() {
        let metin = kart_uret(&ornek_tablo()).metin();
        assert!(metin.contains("musteriler"));
        assert!(metin.contains("id INTEGER PK NN"));
        assert!(metin.contains("ad TEXT"));
    }

    #[test]
    fn uzun_kolon_adi_kisaltilir() {
        let mut tablo = ornek_tablo();
        tablo.kolonlar.push(Kolon {
            ad: "cok".repeat(40),
            tur: KolonTuru::Metin,
            not_null: false,
            birincil_anahtar: false,
            varsayilan: None,
        });
        let metin = kart_uret(&tablo).metin();
        assert!(metin.contains('~'));
    }

    #[test]
    fn iliskiler_yazilir() {
        let mut tablo = ornek_tablo();
        tablo.ad = "siparisler".into();
        tablo.yabancilar = vec![YabanciAnahtar {
            kolonlar: vec!["musteri_id".into()],
            hedef_tablo: "musteriler".into(),
            hedef_kolonlar: vec!["id".into()],
        }];
        let sema = Sema {
            tablolar: vec![ornek_tablo(), tablo],
            indeksler: Vec::new(),
        };
        let satirlar = iliski_satirlari(&sema);
        assert_eq!(satirlar.len(), 1);
        assert!(satirlar[0].contains("musteriler [1] --- [N] siparisler"));
        assert!(satirlar[0].contains("musteri_id"));
        let diyagram = diyagram_uret(&sema);
        assert!(diyagram.contains("== Iliskiler =="));
    }

    #[test]
    fn ozet_satirlari_bilgi_tasiyor() {
        let sema = Sema {
            tablolar: vec![ornek_tablo()],
            indeksler: Vec::new(),
        };
        let satirlar = ozet_satirlari(&sema);
        assert_eq!(satirlar.len(), 1);
        assert!(satirlar[0].contains("2 kolon"));
        assert!(satirlar[0].contains("2 satir"));
        assert!(satirlar[0].contains("kok sayfa 2"));
    }

    #[test]
    fn ortalama_kisaltirilmis_metni_bozmaz() {
        assert_eq!(ortala("ab", 6), "  ab  ");
        assert_eq!(ortala("abcd", 4), "abcd");
    }
}
