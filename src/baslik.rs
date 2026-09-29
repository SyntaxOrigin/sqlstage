//! Veritabanı başlığının (ilk 100 bayt) ayrıştırılması ve doğrulanması.
//!
//! Başlık, dosya biçiminin tek sabit yapısıdır: sihirli sabit, sayfa boyutu, yazma ve
//! okuma sürümü, rezerve alan, kodlama, sayfa sayacı ve b-tree kök sayfası burada durur.
//! Tüm alanlar dosya biçimi belgesindeki bayt ofsetlerine birebir uyar.
//!
//! Kaynak: <https://www.sqlite.org/fileformat2.html#the_database_header>

use crate::hata::SahneHata;

/// Başlık sihirli sabitinin bayt cinsinden uzunluğu: `SQLite format 3\0`.
pub const BASLIK_UZUNLUGU: usize = 100;

/// Başlıkta bildirilebilecek metin kodlamaları.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetinKodlamasi {
    /// UTF-8 (SQLite'in varsayılanı ve tek modern seçeneği).
    Utf8,
    /// UTF-16LE.
    Utf16Le,
    /// UTF-16BE.
    Utf16Be,
}

impl MetinKodlamasi {
    /// Başlık alanındaki sayısal koddan kodlamayı üretir.
    ///
    /// # Hatalar
    ///
    /// Kod 1, 2 veya 3 değilse [`SahneHata::KodlamaDesteklenmiyor`] döner.
    pub fn koddan(kod: u64) -> Result<Self, SahneHata> {
        match kod {
            1 => Ok(Self::Utf8),
            2 => Ok(Self::Utf16Le),
            3 => Ok(Self::Utf16Be),
            diger => Err(SahneHata::KodlamaDesteklenmiyor { kod: diger }),
        }
    }

    /// Kodlamanın başlktaki sayısal kodunu döndürür.
    pub fn kod(&self) -> u64 {
        match self {
            Self::Utf8 => 1,
            Self::Utf16Le => 2,
            Self::Utf16Be => 3,
        }
    }
}

/// Ayrıştırılmış ve doğrulanmış veritabanı başlığı.
#[derive(Debug, Clone)]
pub struct Baslik {
    /// Kullanılabilir sayfa boyutu (rezerve alan düşülmüş).
    pub sayfa_boyutu: u32,
    /// Dosya sonundaki rezerve alanın bayt sayısı (SQLite'ın kendi kullanımı için).
    pub rezerve: u8,
    /// Metin kodlaması.
    pub kodlama: MetinKodlamasi,
    /// Dosya format yazma sürümü (1 = journal, 2 = WAL).
    pub yazma_versiyonu: u8,
    /// Dosya format okuma sürümü (1 = journal, 2 = WAL).
    pub okuma_versiyonu: u8,
    /// Başlıkta bildirilen toplam sayfa sayısı (0 ise dosya uzunluğundan hesaplanır).
    pub bildirilen_sayfa_sayisi: u32,
    /// B-tree kök sayfasının dosya içindeki bayt ofseti (sayfa 1 için 100).
    pub agac_kok_ofseti: u32,
    /// Şema biçim numarası (desteklenen: 1..=4).
    pub sema_bicim: u32,
    /// Metin kodlama alanındaki ham değer (0 ise UTF-8 varsayılır).
    pub ham_kodlama: u64,
}

impl Baslik {
    /// Bir sayfanın kullanılabilir veri alanının bayt uzunluğu.
    ///
    /// Hücre boyutu hesapları bu değere göre yapılır; SQLite'ın `usableSize` değeridir.
    pub fn kullanilabilir(&self) -> u32 {
        self.sayfa_boyutu - u32::from(self.rezerve)
    }

    /// Başlığı geçerli bir SQLite veritabanından ayrıştırır.
    ///
    /// `veri` dosyanın tamamıdır; en az 100 bayt olmalıdır. Sayfa sayısı dosya
    /// uzunluğundan hesaplanır ve başlıktaki değerle karşılaştırılır.
    ///
    /// # Hatalar
    ///
    /// Dosya çok kısa, sihirli sabit yanlış, sayfa boyutu geçersiz (ikinin kuvveti
    /// olmayan veya 512–65536 aralığı dışında), rezerve alan sayfa boyutundan büyük
    /// ya da kodlama alanı geçersizse hata döner.
    pub fn ayikla(veri: &[u8]) -> Result<Self, SahneHata> {
        if veri.len() < BASLIK_UZUNLUGU {
            return Err(SahneHata::DosyaCokKisa { bayt: veri.len() });
        }
        if &veri[0..16] != b"SQLite format 3\0" {
            let son = veri.len().min(16);
            return Err(SahneHata::MagicHatali {
                bulunan: String::from_utf8_lossy(&veri[0..son]).into_owned(),
            });
        }

        let ham_sayfa = u16::from_be_bytes([veri[16], veri[17]]);
        let sayfa_boyutu: u32 = if sayfa_boyutu_kisaltmasi(ham_sayfa) {
            65536
        } else {
            u32::from(ham_sayfa)
        };
        if !(512..=65536).contains(&sayfa_boyutu) || !sayfa_boyutu.is_power_of_two() {
            return Err(SahneHata::GecersizBaslik {
                alan: "sayfa_boyutu",
                deger: u64::from(sayfa_boyutu),
                sebep: "512 ile 65536 arasında ve ikinin kuvveti olmalı",
            });
        }

        let yazma_versiyonu = veri[18];
        let okuma_versiyonu = veri[19];
        if !(1..=2).contains(&yazma_versiyonu) || !(1..=2).contains(&okuma_versiyonu) {
            return Err(SahneHata::GecersizBaslik {
                alan: "yazma/okuma_surumu",
                deger: u64::from(yazma_versiyonu) * 256 + u64::from(okuma_versiyonu),
                sebep: "yalnızca 1 (journal) ve 2 (WAL) geçerlidir",
            });
        }

        let rezerve = veri[20];
        if u32::from(rezerve) >= sayfa_boyutu {
            return Err(SahneHata::GecersizBaslik {
                alan: "rezerve_alan",
                deger: u64::from(rezerve),
                sebep: "rezerve alan sayfa boyutundan küçük olmalı",
            });
        }

        let ham_kodlama = u32::from_be_bytes([veri[56], veri[57], veri[58], veri[59]]) as u64;
        // Alan sıfırsa SQLite varsayılanı (UTF-8) kullanır; 1..3 açık kodlamalardır.
        let kodlama = if ham_kodlama == 0 {
            MetinKodlamasi::Utf8
        } else {
            MetinKodlamasi::koddan(ham_kodlama)?
        };

        let sema_bicim = u32::from_be_bytes([veri[44], veri[45], veri[46], veri[47]]);
        if sema_bicim == 0 || sema_bicim > 4 {
            return Err(SahneHata::GecersizBaslik {
                alan: "sema_bicimi",
                deger: u64::from(sema_bicim),
                sebep: "desteklenen aralık 1..=4",
            });
        }

        let bildirilen_sayfa_sayisi = u32::from_be_bytes([veri[28], veri[29], veri[30], veri[31]]);
        let gercek_sayfa = (veri.len() / sayfa_boyutu as usize) as u32;
        if bildirilen_sayfa_sayisi > 0 && bildirilen_sayfa_sayisi > gercek_sayfa {
            return Err(SahneHata::GecersizBaslik {
                alan: "sayfa_sayisi",
                deger: u64::from(bildirilen_sayfa_sayisi),
                sebep: "başlık sayfa sayısı dosya uzunluğundan büyük",
            });
        }

        let agac_kok_ofseti = if sayfa_boyutu == 65536 {
            0
        } else {
            BASLIK_UZUNLUGU as u32
        };

        Ok(Self {
            sayfa_boyutu,
            rezerve,
            kodlama,
            yazma_versiyonu,
            okuma_versiyonu,
            bildirilen_sayfa_sayisi,
            agac_kok_ofseti,
            sema_bicim,
            ham_kodlama,
        })
    }
}

/// Başlıktaki 16 bitlik ham sayfa boyutu alanı geçerli mi?
///
/// Alan `1` değeri 65536 sayfa boyutunun kısaltmasıdır; `0` ise geçersizdir.
fn sayfa_boyutu_kisaltmasi(ham: u16) -> bool {
    ham == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testlerde kullanılan geçerli bir 512 baytlık başlık.
    fn gecerli_baslik() -> Vec<u8> {
        let mut veri = vec![0u8; 512];
        veri[0..16].copy_from_slice(b"SQLite format 3\0");
        veri[16..18].copy_from_slice(&512u16.to_be_bytes());
        veri[18] = 1;
        veri[19] = 1;
        veri[20] = 0;
        veri[21] = 64;
        veri[22] = 32;
        veri[23] = 32;
        veri[28..32].copy_from_slice(&1u32.to_be_bytes());
        veri[44..48].copy_from_slice(&4u32.to_be_bytes());
        veri[56..60].copy_from_slice(&1u32.to_be_bytes());
        veri
    }

    #[test]
    fn gecerli_baslik_ayristirilir() {
        let baslik = match Baslik::ayikla(&gecerli_baslik()) {
            Ok(b) => b,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(baslik.sayfa_boyutu, 512);
        assert_eq!(baslik.rezerve, 0);
        assert_eq!(baslik.kodlama, MetinKodlamasi::Utf8);
        assert_eq!(baslik.sema_bicim, 4);
        assert_eq!(baslik.kullanilabilir(), 512);
        assert_eq!(baslik.agac_kok_ofseti, 100);
    }

    #[test]
    fn kisa_dosya_reddedilir() {
        let hata = match Baslik::ayikla(&[0u8; 40]) {
            Ok(_) => panic!("kısa dosya kabul edilmemeliydi"),
            Err(hata) => hata,
        };
        assert!(matches!(hata, SahneHata::DosyaCokKisa { bayt: 40 }));
    }

    #[test]
    fn yanlis_sihirli_sabit_reddedilir() {
        let mut veri = gecerli_baslik();
        for bayt in veri[0..16].iter_mut() {
            *bayt = b'X';
        }
        let hata = match Baslik::ayikla(&veri) {
            Ok(_) => panic!("yanlış sihirli sabit kabul edilmemeliydi"),
            Err(hata) => hata,
        };
        assert!(matches!(hata, SahneHata::MagicHatali { .. }));
    }
    #[test]
    fn gecersiz_sayfa_boyutlari_reddedilir() {
        for boyut in [0u16, 3, 513, 1000, 65535] {
            let mut veri = gecerli_baslik();
            veri[16..18].copy_from_slice(&boyut.to_be_bytes());
            let sonuc = Baslik::ayikla(&veri);
            assert!(sonuc.is_err(), "sayfa boyutu {boyut} kabul edilmemeliydi");
        }
    }

    #[test]
    fn sayfa_boyutu_bir_65536_anlamina_gelir() {
        let mut veri = gecerli_baslik();
        veri[16..18].copy_from_slice(&1u16.to_be_bytes());
        veri.resize(65536, 0);
        veri[28..32].copy_from_slice(&1u32.to_be_bytes());
        let baslik = match Baslik::ayikla(&veri) {
            Ok(b) => b,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(baslik.sayfa_boyutu, 65536);
        assert_eq!(baslik.agac_kok_ofseti, 0);
    }

    #[test]
    fn gecersiz_kodlama_reddedilir() {
        for kod in [4u32, 5, 1000] {
            let mut veri = gecerli_baslik();
            veri[56..60].copy_from_slice(&kod.to_be_bytes());
            let sonuc = Baslik::ayikla(&veri);
            assert!(sonuc.is_err(), "kodlama {kod} kabul edilmemeliydi");
        }
    }

    #[test]
    fn kodlama_sifir_utf8_varsayilir() {
        let mut veri = gecerli_baslik();
        veri[56..60].copy_from_slice(&0u32.to_be_bytes());
        let baslik = match Baslik::ayikla(&veri) {
            Ok(b) => b,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(baslik.kodlama, MetinKodlamasi::Utf8);
        assert_eq!(baslik.ham_kodlama, 0);
    }

    #[test]
    fn utf16_kodlamalari_ayristirilir() {
        for (kod, beklenen) in [
            (2u32, MetinKodlamasi::Utf16Le),
            (3, MetinKodlamasi::Utf16Be),
        ] {
            let mut veri = gecerli_baslik();
            veri[56..60].copy_from_slice(&kod.to_be_bytes());
            let baslik = match Baslik::ayikla(&veri) {
                Ok(b) => b,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
            assert_eq!(baslik.kodlama, beklenen);
            assert_eq!(baslik.kodlama.kod(), u64::from(kod));
        }
    }

    #[test]
    fn rezerve_alan_kullanilabilir_yuzeyi_azaltir() {
        let mut veri = gecerli_baslik();
        veri[16..18].copy_from_slice(&512u16.to_be_bytes());
        veri[20] = 32;
        let baslik = match Baslik::ayikla(&veri) {
            Ok(b) => b,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(baslik.rezerve, 32);
        assert_eq!(baslik.kullanilabilir(), 480);
    }

    #[test]
    fn sayfa_sayisi_dosyadan_kucuk_olmalidir() {
        let mut veri = gecerli_baslik();
        veri[28..32].copy_from_slice(&99u32.to_be_bytes());
        let sonuc = Baslik::ayikla(&veri);
        assert!(sonuc.is_err());
    }

    #[test]
    fn sema_bicimi_sifir_reddedilir() {
        let mut veri = gecerli_baslik();
        veri[44..48].copy_from_slice(&0u32.to_be_bytes());
        let sonuc = Baslik::ayikla(&veri);
        assert!(sonuc.is_err());
    }

    #[test]
    fn gecersiz_yazma_surumu_reddedilir() {
        let mut veri = gecerli_baslik();
        veri[18] = 7;
        let sonuc = Baslik::ayikla(&veri);
        assert!(sonuc.is_err());
    }
}
