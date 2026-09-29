//! Salt okunur veritabanı erişimi: dosya açma, `sqlite_master` okuma, tablo taraması ve
//! sayfa bütünlüğü denetimi.
//!
//! Dosya **her zaman** `std::fs::File::open` ile yalnızca okunabilir biçimde açılır ve
//! tüm içerik belleğe alınır. Yazma yolu hiçbir yerde bulunmadığından bir `.db` dosyası
//! bu modül üzerinden değiştirilemez.
//!
//! Kaynak: <https://www.sqlite.org/fileformat2.html>

use std::path::{Path, PathBuf};

use crate::baslik::Baslik;
use crate::deger::Deger;
use crate::hata::SahneHata;
use crate::kayit::kayit_ayikla;
use crate::sayfa::{sayfa_ayikla, BTreeSayfasi, Hucre, SayfaTuru};

/// Bir tablonun b-tree ağacında dolaşılırken kullanılan üst sınılama.
const AZAMI_AGAC_DERINLIGI: usize = 64;

/// Tek bir satır: satır kimliği ve kolon değerleri.
#[derive(Debug, Clone, PartialEq)]
pub struct Satir {
    /// SQLite'ın örtük satır kimliği (`rowid`).
    pub satir_kimligi: i64,
    /// Kolon değerleri, `CREATE TABLE` sırasına göre.
    pub degerler: Vec<Deger>,
}

impl Satir {
    /// Kolon dizininden değeri döndürür (dizin sınır dışıysa `None`).
    pub fn deger(&self, indeks: usize) -> Option<&Deger> {
        self.degerler.get(indeks)
    }
}

/// Bir tablonun okuyucu tarafındaki özeti.
#[derive(Debug, Clone, PartialEq)]
pub struct TabloOzeti {
    /// Tablo adı.
    pub ad: String,
    /// Tablo b-tree'sinin kök sayfa numarası.
    pub kok_sayfa: u32,
    /// `sqlite_master.sql` alanındaki `CREATE TABLE` metni (yoksa boş).
    pub sql: String,
    /// Yaprak sayfalardaki hücre sayısı, yani satır sayısı.
    pub satir_sayisi: u64,
    /// Taramada ziyaret edilen toplam b-tree sayfası (iç + yaprak).
    pub taranan_sayfa: u32,
    /// Hücrelerin bildirdiği toplam yük boyutu (bayt).
    pub yuk_toplami: u64,
}

/// Sayfa bütünlüğü denetiminin sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButunlukRaporu {
    /// Denetlenen toplam sayfa sayısı.
    pub denetlenen_sayfa: u32,
    /// Denetimde ortaya çıkan sorun sayısı (0 ise dosya sağlam kabul edilir).
    pub sorun_sayisi: u32,
    /// Sorunların insan tarafından okunabilir açıklamaları.
    pub sorunlar: Vec<String>,
}

impl ButunlukRaporu {
    /// Denetimin temiz geçip geçmediğini döndürür.
    pub fn temiz_mi(&self) -> bool {
        self.sorun_sayisi == 0
    }
}

/// Salt okunur açılmış bir SQLite veritabanı dosyası.
pub struct Veritabani {
    yol: PathBuf,
    veri: Vec<u8>,
    baslik: Baslik,
}

impl Veritabani {
    /// Bir `.db` dosyasını salt okunur biçimde açar ve başlığını doğrular.
    ///
    /// # Hatalar
    ///
    /// Dosya okunamazsa, 100 bayttan kısaysa, sihirli sabit eşleşmezse veya başlık
    /// alanları geçersizse hata döner.
    pub fn ac(yol: &Path) -> Result<Self, SahneHata> {
        let veri = std::fs::read(yol).map_err(|kaynak| SahneHata::Io {
            yol: yol.display().to_string(),
            kaynak,
        })?;
        let baslik = Baslik::ayikla(&veri)?;
        Ok(Self {
            yol: yol.to_path_buf(),
            veri,
            baslik,
        })
    }

    /// Açılan dosyanın yolunu döndürür.
    pub fn yol(&self) -> &Path {
        &self.yol
    }

    /// Doğrulanmış başlığa erişim sağlar (sayfa boyutu, kodlama, sürüm...).
    pub fn baslik(&self) -> &Baslik {
        &self.baslik
    }

    /// Dosyadaki toplam sayfa sayısını döndürür.
    pub fn sayfa_sayisi(&self) -> u32 {
        (self.veri.len() / self.baslik.sayfa_boyutu as usize) as u32
    }

    /// WAL modunda olup olmadığını döndürür.
    ///
    /// WAL modunda son commit edilmemiş sayfalar `-wal` dosyasındadır; bu okuyucu o
    /// dosyayı açmadığı için görünmez.
    pub fn wal_etkin_mi(&self) -> bool {
        self.baslik.yazma_versiyonu == 2 || self.baslik.okuma_versiyonu == 2
    }

    /// Belirtilen sayfanın kullanılabilir kısmını döndürür.
    ///
    /// # Hatalar
    ///
    /// Sayfa numarası 1'den küçükse, dosyanın son sayfasından büyükse veya sayfa
    /// başlığıyla belirtilen boyuttan kısaysa hata döner.
    pub fn sayfa(&self, sayfa_no: u32) -> Result<Vec<u8>, SahneHata> {
        let sb = self.baslik.sayfa_boyutu as usize;
        if sayfa_no == 0 {
            return Err(SahneHata::SayfaDisinda {
                istenen: sayfa_no,
                mevcut: self.sayfa_sayisi(),
            });
        }
        let bas = (sayfa_no as usize - 1) * sb;
        let son = bas + sb;
        if son > self.veri.len() {
            return Err(SahneHata::SayfaDisinda {
                istenen: sayfa_no,
                mevcut: self.sayfa_sayisi(),
            });
        }
        let dilim = &self.veri[bas..son];
        let kullanilabilir = self.baslik.kullanilabilir() as usize;
        Ok(dilim[..kullanilabilir].to_vec())
    }

    /// Bir b-tree sayfasını ayrıştırır (sayfa 1'de başlık ofseti 100'dür).
    pub fn btree_sayfasi(&self, sayfa_no: u32) -> Result<BTreeSayfasi, SahneHata> {
        let sayfa = self.sayfa(sayfa_no)?;
        let ofset = if sayfa_no == 1 {
            crate::sayfa::BIRINCI_SAYFA_BASLIK_OFSETI
        } else {
            0
        };
        sayfa_ayikla(&sayfa, sayfa_no, ofset, self.baslik.kullanilabilir())
    }

    /// Bir hücrenin tam yükünü (yerel parça + varışma zinciri) okur.
    pub fn hucrenin_yuku(&self, iceren_sayfa: u32, hucre: &Hucre) -> Result<Vec<u8>, SahneHata> {
        crate::kayit::yuk_birlestir(iceren_sayfa, hucre, |no| self.sayfa(no))
    }

    /// Bir hücreyi kayıt değerlerine ayrıştırır.
    pub fn hucrenin_degerleri(
        &self,
        iceren_sayfa: u32,
        hucre: &Hucre,
    ) -> Result<Vec<Deger>, SahneHata> {
        let yuk = self.hucrenin_yuku(iceren_sayfa, hucre)?;
        kayit_ayikla(&yuk, self.baslik.kodlama)
    }

    /// `sqlite_master` tablosundaki tüm satırları ham değerler olarak döndürür.
    ///
    /// Sütun sırası SQLite tarafından sabitlenmiştir:
    /// `type, name, tbl_name, rootpage, sql`.
    pub fn master_satirlari(&self) -> Result<Vec<Vec<Deger>>, SahneHata> {
        let kosul = |ayrinti: String| SahneHata::BozukAgac {
            tablo: "sqlite_master".to_string(),
            ayrinti,
        };
        let mut cikti = Vec::new();
        let mut ziyaret: Vec<u32> = Vec::new();
        self.agac_gez(1, &mut ziyaret, &mut |sayfa_no: u32, hucre: &Hucre| {
            let degerler = self
                .hucrenin_degerleri(sayfa_no, hucre)
                .map_err(|hata| kosul(hata.to_string()))?;
            cikti.push(degerler);
            Ok(())
        })?;
        Ok(cikti)
    }

    /// Verilen kök sayfadan başlayarak tablo b-tree'sinde dolaşır.
    ///
    /// `ziyaret`, aynı sayfanın iki kez işlenmesini (silinmiş sayfa veya döngü) engeller.
    fn agac_gez<F>(
        &self,
        kok_sayfa: u32,
        ziyaret: &mut Vec<u32>,
        yaprak: &mut F,
    ) -> Result<(), SahneHata>
    where
        F: FnMut(u32, &Hucre) -> Result<(), SahneHata>,
    {
        self.agac_gez_derinlik(kok_sayfa, ziyaret, 0, yaprak)
    }

    fn agac_gez_derinlik<F>(
        &self,
        sayfa_no: u32,
        ziyaret: &mut Vec<u32>,
        derinlik: usize,
        yaprak: &mut F,
    ) -> Result<(), SahneHata>
    where
        F: FnMut(u32, &Hucre) -> Result<(), SahneHata>,
    {
        if derinlik > AZAMI_AGAC_DERINLIGI {
            return Err(SahneHata::BozukAgac {
                tablo: format!("sayfa {sayfa_no}"),
                ayrinti: format!("ağaç derinliği {AZAMI_AGAC_DERINLIGI} sınırını aştı"),
            });
        }
        if ziyaret.contains(&sayfa_no) {
            return Err(SahneHata::BozukAgac {
                tablo: format!("sayfa {sayfa_no}"),
                ayrinti: "aynı sayfa iki kez ziyaret edildi (döngü)".to_string(),
            });
        }
        ziyaret.push(sayfa_no);
        let sayfa = self.btree_sayfasi(sayfa_no)?;
        match sayfa.tur {
            SayfaTuru::TabloYaprak => {
                for hucre in &sayfa.hucreler {
                    yaprak(sayfa_no, hucre)?;
                }
            }
            SayfaTuru::TabloIc => {
                for hucre in &sayfa.hucreler {
                    if let Some(cocuk) = hucre.sol_cocuk {
                        self.agac_gez_derinlik(cocuk, ziyaret, derinlik + 1, yaprak)?;
                    }
                }
                if let Some(sag) = sayfa.sag_cocuk {
                    if sag != 0 {
                        self.agac_gez_derinlik(sag, ziyaret, derinlik + 1, yaprak)?;
                    }
                }
            }
            diger => {
                return Err(SahneHata::BozukAgac {
                    tablo: format!("sayfa {sayfa_no}"),
                    ayrinti: format!("tablo b-tree içinde {diger:?} türünde sayfa bulundu"),
                })
            }
        }
        Ok(())
    }

    /// Bir tablonun yaprak sayfalarını yalnızca başlık düzeyinde dolaşır.
    ///
    /// Hücre yükleri okunmaz; yalnızca sayfa/hücre sayıları sayılır. Sorgu planı maliyet
    /// tahmini bu ucuz dolaşımı kullanır.
    pub fn tablo_yapisini_gez(&self, kok_sayfa: u32) -> Result<(u64, u32, u64), SahneHata> {
        let mut hucre_sayisi = 0u64;
        let mut sayfa_sayisi = 0u32;
        let mut yuk_toplami = 0u64;
        let mut ziyaret = Vec::new();
        self.yapi_gez(kok_sayfa, &mut ziyaret, 0, &mut |sayfa: &BTreeSayfasi| {
            hucre_sayisi += u64::from(sayfa.hucre_sayisi);
            sayfa_sayisi += 1;
            yuk_toplami += sayfa.bildirilen_yuk_toplami();
        })?;
        Ok((hucre_sayisi, sayfa_sayisi, yuk_toplami))
    }

    fn yapi_gez<F>(
        &self,
        sayfa_no: u32,
        ziyaret: &mut Vec<u32>,
        derinlik: usize,
        yaprak: &mut F,
    ) -> Result<(), SahneHata>
    where
        F: FnMut(&BTreeSayfasi),
    {
        if derinlik > AZAMI_AGAC_DERINLIGI {
            return Err(SahneHata::BozukAgac {
                tablo: format!("sayfa {sayfa_no}"),
                ayrinti: format!("ağaç derinliği {AZAMI_AGAC_DERINLIGI} sınırını aştı"),
            });
        }
        if ziyaret.contains(&sayfa_no) {
            return Err(SahneHata::BozukAgac {
                tablo: format!("sayfa {sayfa_no}"),
                ayrinti: "aynı sayfa iki kez ziyaret edildi (döngü)".to_string(),
            });
        }
        ziyaret.push(sayfa_no);
        let sayfa = self.btree_sayfasi(sayfa_no)?;
        if sayfa.tur.indeks_mi() {
            return Err(SahneHata::BozukAgac {
                tablo: format!("sayfa {sayfa_no}"),
                ayrinti: "tablo b-tree içinde indeks sayfası bulundu".to_string(),
            });
        }
        if sayfa.tur.ic_sayfa_mi() {
            for hucre in &sayfa.hucreler {
                if let Some(cocuk) = hucre.sol_cocuk {
                    self.yapi_gez(cocuk, ziyaret, derinlik + 1, yaprak)?;
                }
            }
            if let Some(sag) = sayfa.sag_cocuk {
                if sag != 0 {
                    self.yapi_gez(sag, ziyaret, derinlik + 1, yaprak)?;
                }
            }
        }
        yaprak(&sayfa);
        Ok(())
    }

    /// Bir tablonun tüm satırlarını okur.
    ///
    /// # Hatalar
    ///
    /// Kök sayfa dosya sınırı dışındaysa, ağaç bozuksa veya bir kayıt çözülemiyorsa hata
    /// döner.
    pub fn tablo_satirlari(&self, kok_sayfa: u32) -> Result<Vec<Satir>, SahneHata> {
        let mut satirlar: Vec<Satir> = Vec::new();
        let mut ziyaret: Vec<u32> = Vec::new();
        self.agac_gez(kok_sayfa, &mut ziyaret, &mut |sayfa_no, hucre| {
            let degerler = self.hucrenin_degerleri(sayfa_no, hucre)?;
            satirlar.push(Satir {
                satir_kimligi: hucre.anahtar,
                degerler,
            });
            Ok(())
        })?;
        Ok(satirlar)
    }

    /// Başlıktaki tablo listesini (indeksler hariç) döndürür.
    ///
    /// `sqlite_master` bozuksa hata döner; yalnızca indeks satırları varsa boş liste döner.
    pub fn tablo_ozetleri(&self) -> Result<Vec<TabloOzeti>, SahneHata> {
        let mut ozetler = Vec::new();
        for satir in self.master_satirlari()? {
            let tur = satir.first().and_then(Deger::metin).unwrap_or("");
            if tur != "table" {
                continue;
            }
            let ad = satir
                .get(1)
                .and_then(Deger::metin)
                .unwrap_or("")
                .to_string();
            let kok_sayfa = match satir.get(3) {
                Some(Deger::Tam(k)) if *k > 0 => *k as u32,
                _ => {
                    return Err(SahneHata::SemaHatasi {
                        mesaj: format!("tablo \"{ad}\" için geçerli bir kök sayfa numarası yok"),
                    })
                }
            };
            let sql = satir
                .get(4)
                .and_then(Deger::metin)
                .unwrap_or("")
                .to_string();
            let (satir_sayisi, taranan_sayfa, yuk_toplami) = self.tablo_yapisini_gez(kok_sayfa)?;
            ozetler.push(TabloOzeti {
                ad,
                kok_sayfa,
                sql,
                satir_sayisi,
                taranan_sayfa,
                yuk_toplami,
            });
        }
        Ok(ozetler)
    }

    /// Tüm sayfaların b-tree düzenini denetler.
    ///
    /// Denetim başlık, hücre işaretçisi ve varışma zinciri sınırlarını kontrol eder. Bu,
    /// `PRAGMA integrity_check` yerine geçen hafif bir denetlemdir: indeks ağaçlarını
    /// gezmez, yalnızca `sqlite_master` ve kullanıcı tabloları kapsanır.
    pub fn butunluk(&self) -> Result<ButunlukRaporu, SahneHata> {
        let mut sorunlar: Vec<String> = Vec::new();
        // 1. sayfa şema ağacının köküdür ve `master_satirlari` ile okunmuştur.
        let mut denetlenen = 1u32;
        let mut koklar: Vec<(String, u32)> = Vec::new();
        match self.master_satirlari() {
            Ok(satirlar) => {
                for satir in satirlar {
                    let tur = satir
                        .first()
                        .and_then(Deger::metin)
                        .unwrap_or("")
                        .to_string();
                    if tur != "table" {
                        continue;
                    }
                    let ad = satir
                        .get(1)
                        .and_then(Deger::metin)
                        .unwrap_or("")
                        .to_string();
                    if let Some(Deger::Tam(k)) = satir.get(3) {
                        if *k > 0 {
                            koklar.push((ad, *k as u32));
                        }
                    }
                }
            }
            Err(hata) => sorunlar.push(format!("sqlite_master okunamadı: {hata}")),
        }
        for (ad, kok) in koklar {
            let mut ziyaret = Vec::new();
            match self.yapi_gez(kok, &mut ziyaret, 0, &mut |sayfa: &BTreeSayfasi| {
                denetlenen += 1;
                if sayfa.parcalanmis_bos_bayt > 60 {
                    sorunlar.push(format!(
                        "sayfa {} parçalanmış boş bayt sayısı {}",
                        kok, sayfa.parcalanmis_bos_bayt
                    ));
                }
            }) {
                Ok(()) => {}
                Err(hata) => sorunlar.push(format!("tablo \"{ad}\": {hata}")),
            }
        }
        Ok(ButunlukRaporu {
            denetlenen_sayfa: denetlenen,
            sorun_sayisi: sorunlar.len() as u32,
            sorunlar,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yazici::{veritabani_goruntusu_sayfa_boyutu, TabloYazimi, TEST_SAYFA_BOYUTU};

    fn gecici_yol(etiket: &str) -> PathBuf {
        let dizin =
            std::env::temp_dir().join(format!("sqlstage-okuyucu-{}-{etiket}", std::process::id()));
        let _ = std::fs::create_dir_all(&dizin);
        dizin.join("test.db")
    }

    fn ornek_goruntu() -> Vec<u8> {
        let tablolar = vec![TabloYazimi {
            ad: "kisi".to_string(),
            sql: "CREATE TABLE kisi(id INTEGER PRIMARY KEY, ad TEXT, yas INTEGER)".to_string(),
            satirlar: vec![
                (
                    1,
                    vec![Deger::Tam(1), Deger::Metin("Ali".into()), Deger::Tam(30)],
                ),
                (
                    2,
                    vec![Deger::Tam(2), Deger::Metin("Veli".into()), Deger::Null],
                ),
            ],
        }];
        match veritabani_goruntusu_sayfa_boyutu(&tablolar, TEST_SAYFA_BOYUTU) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    fn ac_ornek(yol: &Path) -> Veritabani {
        std::fs::write(yol, ornek_goruntu()).unwrap_or_default();
        match Veritabani::ac(yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    #[test]
    fn dosya_acma_ve_baslik_okuma() {
        let yol = gecici_yol("ac");
        let db = ac_ornek(&yol);
        assert_eq!(db.baslik().sayfa_boyutu, TEST_SAYFA_BOYUTU);
        assert!(!db.wal_etkin_mi());
        assert!(db.sayfa_sayisi() >= 2);
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn olmayan_dosya_hata_verir() {
        let sonuc = Veritabani::ac(Path::new("C:/sqlstage-yok-boyle-bir-dosya.db"));
        assert!(matches!(sonuc, Err(SahneHata::Io { .. })));
    }

    #[test]
    fn sifir_numarali_sayfa_reddedilir() {
        let yol = gecici_yol("sifir");
        let db = ac_ornek(&yol);
        assert!(matches!(
            db.sayfa(0),
            Err(SahneHata::SayfaDisinda { istenen: 0, .. })
        ));
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn dosya_sonu_sonrasi_sayfa_reddedilir() {
        let yol = gecici_yol("sonras");
        let db = ac_ornek(&yol);
        assert!(db.sayfa(db.sayfa_sayisi() + 10).is_err());
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn tablo_satirlari_okunur() {
        let yol = gecici_yol("satir");
        let db = ac_ornek(&yol);
        let ozetler = match db.tablo_ozetleri() {
            Ok(o) => o,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(ozetler.len(), 1);
        assert_eq!(ozetler[0].ad, "kisi");
        assert_eq!(ozetler[0].satir_sayisi, 2);
        let satirlar = match db.tablo_satirlari(ozetler[0].kok_sayfa) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(satirlar.len(), 2);
        assert_eq!(satirlar[0].satir_kimligi, 1);
        assert_eq!(satirlar[1].deger(2), Some(&Deger::Null));
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn bos_tablo_sifir_satir_doner() {
        let yol = gecici_yol("bos");
        let goruntu = match veritabani_goruntusu_sayfa_boyutu(
            &[TabloYazimi {
                ad: "bos".into(),
                sql: "CREATE TABLE bos(id INTEGER)".into(),
                satirlar: Vec::new(),
            }],
            TEST_SAYFA_BOYUTU,
        ) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        std::fs::write(&yol, goruntu).unwrap_or_default();
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let ozetler = match db.tablo_ozetleri() {
            Ok(o) => o,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(ozetler[0].satir_sayisi, 0);
        assert!(db
            .tablo_satirlari(ozetler[0].kok_sayfa)
            .unwrap_or_default()
            .is_empty());
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn butunluk_saglam_dosyada_temiz() {
        let yol = gecici_yol("butunluk");
        let db = ac_ornek(&yol);
        let rapor = match db.butunluk() {
            Ok(r) => r,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(rapor.temiz_mi(), "{:?}", rapor.sorunlar);
        assert!(rapor.denetlenen_sayfa >= 2);
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn bozuk_sayfa_tipi_butunluk_denetinde_yakalanir() {
        let yol = gecici_yol("bozuk");
        let mut goruntu = ornek_goruntu();
        // 2. sayfa tablo köküdür; tür baytını geçersizleştir.
        let kok = TEST_SAYFA_BOYUTU as usize;
        goruntu[kok] = 0x00;
        std::fs::write(&yol, goruntu).unwrap_or_default();
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let rapor = match db.butunluk() {
            Ok(r) => r,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(!rapor.temiz_mi());
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn indeks_tipi_tablo_agacinda_reddedilir() {
        let yol = gecici_yol("indeks");
        let mut goruntu = ornek_goruntu();
        goruntu[TEST_SAYFA_BOYUTU as usize] = 0x0A; // indeks yaprak sayfası
        std::fs::write(&yol, goruntu).unwrap_or_default();
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let sonuc = db.tablo_ozetleri();
        assert!(sonuc.is_err());
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn kisa_dosya_acilmaz() {
        let yol = gecici_yol("kisa");
        std::fs::write(&yol, b"SQLite format 3\0").unwrap_or_default();
        let sonuc = Veritabani::ac(&yol);
        assert!(matches!(sonuc, Err(SahneHata::DosyaCokKisa { .. })));
        let _ = std::fs::remove_file(&yol);
    }

    #[test]
    fn yol_bilgisi_dondurulur() {
        let yol = gecici_yol("yol");
        let db = ac_ornek(&yol);
        assert_eq!(db.yol(), yol.as_path());
        let _ = std::fs::remove_file(&yol);
    }
}
