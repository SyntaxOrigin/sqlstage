//! Minimal SQLite dosya **yazıcısı**.
//!
//! Bu modül yalnızca **kullanıcının açmadığı yeni** örnek veritabanı dosyaları üretmek
//! içindir (`sample`, `create`, `insert`). Okunan bir veritabanı dosyası bu yolla
//! değiştirilmez: `insert` bile önce dosyanın tamamını okuyup bir modele çevirir, sonra
//! dosyayı baştan yazar; indeks, `WITHOUT ROWID` veya okunamayan bir yapı varsa yazma
//! başlamadan reddedilir.
//!
//! Desteklenen yazma alt kümesi: sabit sayfa boyutu (varsayılan 4096), yalnızca satır
//! kimlikli (rowid) tablolar, satır içi + varışma (overflow) yükleri, indeks yok.
//!
//! Kaynak: <https://www.sqlite.org/fileformat2.html>

use std::fs;
use std::path::{Path, PathBuf};

use crate::deger::Deger;
use crate::hata::SahneHata;
use crate::sayfa::SayfaTuru;

/// Üretilen veritabanı dosyalarında kullanılan varsayılan sayfa boyutu.
pub const VARSAYILAN_SAYFA_BOYUTU: u32 = 4096;

/// Başlığa yazılan SQLite dosya biçimi sürümü (yalnızca bilgi amaçlıdır).
const DOSYA_BICIM_SURUMU: u32 = 3_046_000;

/// Testlerin küçük sayfalarla varışma yolunu zorlamak için kullandığı sayfa boyutu.
pub const TEST_SAYFA_BOYUTU: u32 = 512;

/// Yazılacak bir tablonun tam tanımı: adı, `CREATE TABLE` metni ve satır kimlikli satırları.
#[derive(Debug, Clone, PartialEq)]
pub struct TabloYazimi {
    /// Tablo adı (`sqlite_master.name` ve `tbl_name` alanlarına yazılır).
    pub ad: String,
    /// `sqlite_master.sql` alanına yazılacak tam `CREATE TABLE` ifadesi.
    pub sql: String,
    /// `(satır kimliği, kolon değerleri)` çiftleri; sıra önemsizdir, yazıcı sıralar.
    pub satirlar: Vec<(i64, Vec<Deger>)>,
}

/// Ağaç inşasında kullanılan tek bir hücre girdisi.
struct HucreGirdi {
    anahtar: i64,
    baytlar: Vec<u8>,
}

/// Bir düzeydeki düğümlerin kapsadığı **yaprak** aralığı (yarı açık: `bas` dahil, `bitis` hariç).
type YaprakAraligi = (usize, usize);

/// Sayfa havuzunu tutan yazıcı.
struct SayfaHavuzu {
    sayfalar: Vec<Vec<u8>>,
    sayfa_boyutu: u32,
}

impl SayfaHavuzu {
    fn yeni(sayfa_boyutu: u32) -> Self {
        Self {
            sayfalar: vec![vec![0u8; sayfa_boyutu as usize]],
            sayfa_boyutu,
        }
    }

    /// Yeni bir sayfa ayırır ve numarasını döndürür (1 tabanlı).
    fn sayfa_ayir(&mut self) -> u32 {
        self.sayfalar.push(vec![0u8; self.sayfa_boyutu as usize]);
        self.sayfalar.len() as u32
    }

    /// Bir tablo b-tree yaprak hücresi üretir; gerekirse varışma sayfaları ayırır.
    fn tablo_hucresi(&mut self, rowid: i64, degerler: &[Deger]) -> HucreGirdi {
        let payload = crate::kayit::kayit_yaz(degerler);
        let (yerel, varisma) =
            crate::sayfa::yuk_bol(payload.len(), SayfaTuru::TabloYaprak, self.sayfa_boyutu);
        let mut hucre = Vec::with_capacity(yerel + 26);
        hucre.extend(crate::varint::varint_yaz(payload.len() as i64));
        hucre.extend(crate::varint::varint_yaz(rowid));
        hucre.extend_from_slice(&payload[..yerel]);
        if varisma {
            hucre.extend_from_slice(&self.varisma_ayir(&payload[yerel..]).to_be_bytes());
        }
        HucreGirdi {
            anahtar: rowid,
            baytlar: hucre,
        }
    }

    /// Kalan yükü varışma sayfalarına yazar ve ilk sayfanın numarasını döndürür.
    fn varisma_ayir(&mut self, veri: &[u8]) -> u32 {
        let kapasite = (self.sayfa_boyutu - 4) as usize;
        let ilk = self.sayfa_ayir();
        let mut mevcut = ilk;
        let mut kalan = veri;
        while !kalan.is_empty() {
            let alinacak = kapasite.min(kalan.len());
            let sonraki = if alinacak < kalan.len() {
                self.sayfa_ayir()
            } else {
                0
            };
            let sayfa = &mut self.sayfalar[(mevcut - 1) as usize];
            sayfa[..4].copy_from_slice(&sonraki.to_be_bytes());
            sayfa[4..4 + alinacak].copy_from_slice(&kalan[..alinacak]);
            mevcut = sonraki;
            kalan = &kalan[alinacak..];
        }
        ilk
    }

    /// Hücreleri bir sayfaya yazar; taşmaya yetecek yer yoksa hata döner.
    fn hucreleri_yaz(
        &mut self,
        sayfa_no: u32,
        tur: SayfaTuru,
        hucreler: &[Vec<u8>],
        sag_cocuk: Option<u32>,
        ofset: usize,
    ) -> Result<(), SahneHata> {
        let baslik_uzunlugu = tur.baslik_uzunlugu();
        let sb = self.sayfa_boyutu as usize;
        let toplam: usize =
            baslik_uzunlugu + hucreler.len() * 2 + hucreler.iter().map(Vec::len).sum::<usize>();
        if ofset + toplam > sb {
            return Err(SahneHata::BozukSayfa {
                sayfa: sayfa_no,
                ayrinti: format!("sayfa {sayfa_no} içine {toplam} bayt sığmıyor"),
            });
        }
        let sayfa = &mut self.sayfalar[(sayfa_no - 1) as usize];
        sayfa[ofset] = tur.bayt();
        sayfa[ofset + 1..ofset + 3].copy_from_slice(&0u16.to_be_bytes());
        sayfa[ofset + 3..ofset + 5].copy_from_slice(&(hucreler.len() as u16).to_be_bytes());
        sayfa[ofset + 7] = 0;
        if tur.ic_sayfa_mi() {
            sayfa[ofset + 8..ofset + 12].copy_from_slice(&sag_cocuk.unwrap_or(0).to_be_bytes());
        }
        let mut icerik = sb;
        for (i, hucre) in hucreler.iter().enumerate() {
            icerik -= hucre.len();
            sayfa[icerik..icerik + hucre.len()].copy_from_slice(hucre);
            let isaretci = ofset + baslik_uzunlugu + i * 2;
            sayfa[isaretci..isaretci + 2].copy_from_slice(&(icerik as u16).to_be_bytes());
        }
        let baslangic = if icerik == 65536 { 0u16 } else { icerik as u16 };
        sayfa[ofset + 5..ofset + 7].copy_from_slice(&baslangic.to_be_bytes());
        Ok(())
    }

    /// Satır kimlikli bir b-tree ağacı yazar ve kök sayfa numarasını döndürür.
    ///
    /// `kok_yerine` `Some(1)` ise ağacın kökü 1. sayfaya (başlıktan sonraki alana) yazılır;
    /// bu, `sqlite_master` için zorunludur. Hücreler artan satır kimliği sırasında olmalıdır.
    ///
    /// Ağaç "yükleme" (bulk-load) ile kurulur: hücreler önce sayfalara paketlenir, sonra
    /// aşağıdan yukarı iç sayfa düzeyleri üretilir. Böylece sayfa bölme (split) mantığına
    /// gerek kalmaz.
    fn agac_yaz(
        &mut self,
        kok_yerine: Option<u32>,
        hucruler: Vec<HucreGirdi>,
    ) -> Result<u32, SahneHata> {
        let sb = self.sayfa_boyutu as usize;
        let kok_ofset = if kok_yerine == Some(1) { 100 } else { 0 };

        // Tek sayfaya sığan ağaç: doğrudan kök sayfaya yazılır (en sık durum).
        let tek_toplam: usize =
            8 + hucruler.len() * 2 + hucruler.iter().map(|h| h.baytlar.len()).sum::<usize>();
        if hucruler.is_empty() || kok_ofset + tek_toplam <= sb {
            let no = match kok_yerine {
                Some(n) => n,
                None => self.sayfa_ayir(),
            };
            let baytlar: Vec<Vec<u8>> = hucruler.iter().map(|h| h.baytlar.clone()).collect();
            self.hucreleri_yaz(no, SayfaTuru::TabloYaprak, &baytlar, None, kok_ofset)?;
            return Ok(no);
        }

        // Düzeyleri, sayfa numarası atamadan önce hesapla.
        let hucre_boyutlari: Vec<usize> = hucruler.iter().map(|h| h.baytlar.len()).collect();
        let mut duzeyler: Vec<Vec<YaprakAraligi>> = vec![duzeyi_bol(&hucre_boyutlari, sb, 8)];
        loop {
            let son_sayisi = duzeyler.last().map_or(0, Vec::len);
            if son_sayisi == 1 {
                break;
            }
            let onceki = duzeyler.last().map_or(&[][..], Vec::as_slice);
            let boyutlar: Vec<usize> = onceki
                .iter()
                .map(|&(_, son)| 4 + crate::varint::varint_uzunluk(hucruler[son - 1].anahtar))
                .collect();
            let yeni = duzeyi_bol(&boyutlar, sb, 12);
            if yeni.len() >= son_sayisi {
                return Err(SahneHata::BozukAgac {
                    tablo: "<agac>".to_string(),
                    ayrinti: "iç sayfa düzeyi ilerleme sağlamıyor".to_string(),
                });
            }
            duzeyler.push(yeni);
        }

        // Sayfa numaraları: kök, en üst düzeyin tek düğümüdür ve isteğe bağlı olarak
        // 1. sayfaya sabitlenebilir (sqlite_master zorunluluğu).
        let tepe = duzeyler.len() - 1;
        let mut sayfalar: Vec<Vec<u32>> = vec![Vec::new(); duzeyler.len()];
        for indeks in 0..tepe {
            for _ in 0..duzeyler[indeks].len() {
                sayfalar[indeks].push(self.sayfa_ayir());
            }
        }
        let kok = match kok_yerine {
            Some(n) => n,
            None => self.sayfa_ayir(),
        };
        sayfalar[tepe] = vec![kok];

        // Sayfaları yaz.
        for indeks in 0..duzeyler.len() {
            for (d, &(yaprak_bas, yaprak_bit)) in duzeyler[indeks].iter().enumerate() {
                let no = sayfalar[indeks][d];
                let ofset = if no == 1 { kok_ofset } else { 0 };
                if indeks == 0 {
                    let baytlar: Vec<Vec<u8>> = hucruler[yaprak_bas..yaprak_bit]
                        .iter()
                        .map(|h| h.baytlar.clone())
                        .collect();
                    self.hucreleri_yaz(no, SayfaTuru::TabloYaprak, &baytlar, None, ofset)?;
                    continue;
                }
                let onceki = &duzeyler[indeks - 1];
                let ilk_dugum = onceki
                    .iter()
                    .position(|&(b, _)| b == yaprak_bas)
                    .unwrap_or(0);
                let son_dugum = onceki
                    .iter()
                    .position(|&(_, b)| b == yaprak_bit)
                    .unwrap_or(0);
                let mut hucreler: Vec<Vec<u8>> = Vec::new();
                for k in ilk_dugum..son_dugum {
                    let cocuk = sayfalar[indeks - 1][k];
                    let anahtar = hucruler[onceki[k].1 - 1].anahtar;
                    let mut hucre = Vec::with_capacity(13);
                    hucre.extend_from_slice(&cocuk.to_be_bytes());
                    hucre.extend(crate::varint::varint_yaz(anahtar));
                    hucreler.push(hucre);
                }
                let sag = sayfalar[indeks - 1][son_dugum];
                self.hucreleri_yaz(no, SayfaTuru::TabloIc, &hucreler, Some(sag), ofset)?;
            }
        }
        Ok(kok)
    }

    /// Tüm sayfaları ve başlığı birleştirip dosya görüntüsünü üretir.
    fn bitir(mut self, sayfa_sayisi: u32) -> Vec<u8> {
        let sb = self.sayfa_boyutu as usize;
        let mut veri = vec![0u8; sb];
        veri[0..16].copy_from_slice(b"SQLite format 3\0");
        let ham = if self.sayfa_boyutu == 65536 {
            1u16
        } else {
            self.sayfa_boyutu as u16
        };
        veri[16..18].copy_from_slice(&ham.to_be_bytes());
        veri[18] = 1; // yazma sürümü: rollback journal
        veri[19] = 1; // okuma sürümü: rollback journal
        veri[20] = 0; // rezerve alan
        veri[21] = 64;
        veri[22] = 32;
        veri[23] = 32;
        veri[24..28].copy_from_slice(&1u32.to_be_bytes()); // değişiklik sayacı
        veri[28..32].copy_from_slice(&sayfa_sayisi.to_be_bytes());
        veri[40..44].copy_from_slice(&1u32.to_be_bytes()); // şema çerezi
        veri[44..48].copy_from_slice(&4u32.to_be_bytes()); // şema biçimi 4
        veri[56..60].copy_from_slice(&1u32.to_be_bytes()); // metin kodlaması UTF-8
        veri[92..96].copy_from_slice(&1u32.to_be_bytes()); // sürüm geçerlilik sayacı
        veri[96..100].copy_from_slice(&DOSYA_BICIM_SURUMU.to_be_bytes());
        self.sayfalar[0][..100].copy_from_slice(&veri[..100]);
        let mut cikti = Vec::with_capacity(sb * self.sayfalar.len());
        for sayfa in self.sayfalar {
            cikti.extend(sayfa);
        }
        cikti
    }
}

/// Bir düzeyin hücreleri sayfalara böler ve her sayfanın kapsadığı aralığı döndürür.
///
/// `boyutlar[i]`, i. düğümün hücre için harcadığı bayt sayısıdır (2 baytlık işaretçi
/// dâhil değildir; o aşağıda eklenir). Bir sayfadaki **son** düğüm hücre olarak yazılmaz,
/// onun yerine sağ-çocuk işaretçisi kullanılır; bu yüzden son düğüm de muhafazakâr biçimde
/// hücre boyutuyla sayılır.
fn duzeyi_bol(boyutlar: &[usize], sb: usize, baslik: usize) -> Vec<YaprakAraligi> {
    let mut araliklar: Vec<YaprakAraligi> = Vec::new();
    let mut bas = 0usize;
    let mut harcanan = 0usize;
    for (i, boyut) in boyutlar.iter().enumerate() {
        let ekleme = boyut + 2;
        if i > bas && baslik + harcanan + ekleme > sb {
            araliklar.push((bas, i));
            bas = i;
            harcanan = 0;
        }
        harcanan += ekleme;
    }
    if !boyutlar.is_empty() {
        araliklar.push((bas, boyutlar.len()));
    }
    araliklar
}

/// Tabloları geçerli bir SQLite dosya görüntüsüne çevirir (dosyaya yazmaz).
///
/// # Hatalar
///
/// Ağaç sayfaları sayfa 2'den itibaren sırayla ayrılır; 1. sayfa yalnızca başlık ve şema
/// ağacının kökü için kullanılır. Bir tablo küçük sayfa boyutunda bile tek hücreye
/// sığmıyorsa hata döner.
pub fn veritabani_goruntusu(tablolar: &[TabloYazimi]) -> Result<Vec<u8>, SahneHata> {
    veritabani_goruntusu_sayfa_boyutu(tablolar, VARSAYILAN_SAYFA_BOYUTU)
}

/// [`veritabani_goruntusu`] işlevini belirtilen sayfa boyutuyla çalıştırır.
///
/// Testler küçük sayfalar kullanarak varışma (overflow) davranışını zorlamak için bu
/// sürümü çağırır.
pub fn veritabani_goruntusu_sayfa_boyutu(
    tablolar: &[TabloYazimi],
    sayfa_boyutu: u32,
) -> Result<Vec<u8>, SahneHata> {
    let mut havuz = SayfaHavuzu::yeni(sayfa_boyutu);

    let mut kok_sayfalar: Vec<u32> = Vec::with_capacity(tablolar.len());
    for tablo in tablolar {
        let mut satirlar = tablo.satirlar.clone();
        satirlar.sort_by_key(|(rowid, _)| *rowid);
        let mut hucruler = Vec::with_capacity(satirlar.len());
        for (rowid, degerler) in &satirlar {
            hucruler.push(havuz.tablo_hucresi(*rowid, degerler));
        }
        let kok = havuz.agac_yaz(None, hucruler)?;
        kok_sayfalar.push(kok);
    }

    // sqlite_master satırları: (type, name, tbl_name, rootpage, sql)
    let mut master_hucruler = Vec::with_capacity(tablolar.len());
    for (sira, tablo) in tablolar.iter().enumerate() {
        let degerler = vec![
            Deger::Metin("table".to_string()),
            Deger::Metin(tablo.ad.clone()),
            Deger::Metin(tablo.ad.clone()),
            Deger::Tam(i64::from(kok_sayfalar[sira])),
            Deger::Metin(tablo.sql.clone()),
        ];
        master_hucruler.push(havuz.tablo_hucresi(sira as i64 + 1, &degerler));
    }
    havuz.agac_yaz(Some(1), master_hucruler)?;

    let sayfa_sayisi = havuz.sayfalar.len() as u32;
    Ok(havuz.bitir(sayfa_sayisi))
}

/// Üretilen dosya görüntüsünü diske yazar (dosya varsa üzerine yazılır).
pub fn veritabani_yaz(yol: &Path, tablolar: &[TabloYazimi]) -> Result<(), SahneHata> {
    let goruntu = veritabani_goruntusu(tablolar)?;
    dosya_guncelle(yol, &goruntu)
}

/// Bir dosyayı "yaz → yedekle → değiştir" sırasıyla günceller.
///
/// Geçici dosya önce yazılır; hedef varsa bir yedek kopyalanır, hedef kaldırılır ve
/// geçici dosya hedefe taşınır. Taşıma başarısız olursa yedek geri yüklenmeye çalışılır.
/// Böylece yazma sırasında oluşan bir hata, kullanıcının veritabanını yarım bırakmaz.
pub fn dosya_guncelle(yol: &Path, veri: &[u8]) -> Result<(), SahneHata> {
    let gecici = gecici_yol(yol, "sqlstage-tmp");
    let yedek = gecici_yol(yol, "sqlstage-bak");
    let _ = fs::remove_file(&gecici);
    let _ = fs::remove_file(&yedek);

    yaz(&gecici, veri)?;
    let var = yol.exists();
    if var {
        kopyala(yol, &yedek)?;
    }
    let sonuc = (|| -> Result<(), SahneHata> {
        if var {
            kaldir(yol)?;
        }
        tasir(&gecici, yol)
    })();
    if let Err(hata) = sonuc {
        let _ = fs::remove_file(&gecici);
        if var {
            let _ = fs::rename(&yedek, yol);
        }
        return Err(hata);
    }
    if var {
        let _ = fs::remove_file(&yedek);
    }
    Ok(())
}

/// Verilen yolun geçici kardeş dosyasını döndürür.
fn gecici_yol(yol: &Path, ek: &str) -> PathBuf {
    PathBuf::from(format!("{}.{ek}", yol.display()))
}

fn yaz(yol: &Path, veri: &[u8]) -> Result<(), SahneHata> {
    fs::write(yol, veri).map_err(|kaynak| SahneHata::Io {
        yol: yol.display().to_string(),
        kaynak,
    })
}

fn kopyala(kaynak: &Path, hedef: &Path) -> Result<(), SahneHata> {
    fs::copy(kaynak, hedef).map_err(|kaynak| SahneHata::Io {
        yol: hedef.display().to_string(),
        kaynak,
    })?;
    Ok(())
}

fn kaldir(yol: &Path) -> Result<(), SahneHata> {
    fs::remove_file(yol).map_err(|kaynak| SahneHata::Io {
        yol: yol.display().to_string(),
        kaynak,
    })
}

fn tasir(kaynak: &Path, hedef: &Path) -> Result<(), SahneHata> {
    fs::rename(kaynak, hedef).map_err(|kaynak| SahneHata::Io {
        yol: hedef.display().to_string(),
        kaynak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ornek_tablo() -> TabloYazimi {
        TabloYazimi {
            ad: "kisi".to_string(),
            sql: "CREATE TABLE kisi(id INTEGER PRIMARY KEY, ad TEXT)".to_string(),
            satirlar: vec![
                (1, vec![Deger::Tam(1), Deger::Metin("Ali".into())]),
                (2, vec![Deger::Tam(2), Deger::Metin("Veli".into())]),
            ],
        }
    }

    #[test]
    fn duzey_bolme_gruplari_kapsar() {
        let boyutlar = vec![100usize; 10];
        let gruplar = duzeyi_bol(&boyutlar, 512, 8);
        assert!(gruplar.len() > 1);
        assert_eq!(gruplar[0].0, 0);
        assert_eq!(gruplar[gruplar.len() - 1].1, 10);
        for pencere in gruplar.windows(2) {
            assert_eq!(pencere[0].1, pencere[1].0, "aralıklar bitişik olmalı");
        }
    }

    #[test]
    fn duzey_bolme_tek_sayfaya_sigarsa_tek_grup() {
        let boyutlar = vec![10usize; 5];
        let gruplar = duzeyi_bol(&boyutlar, 4096, 8);
        assert_eq!(gruplar, vec![(0, 5)]);
    }

    #[test]
    fn duzey_bolme_bos_girdi_bos_sonuc() {
        assert!(duzeyi_bol(&[], 512, 8).is_empty());
    }

    #[test]
    fn bos_tablolar_goruntusu_uretir() {
        let goruntu = match veritabani_goruntusu(&[]) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(goruntu.len(), VARSAYILAN_SAYFA_BOYUTU as usize);
        assert_eq!(&goruntu[0..16], b"SQLite format 3\0");
    }

    #[test]
    fn bos_tablo_tek_yaprak_sayfasi_alir() {
        let goruntu = match veritabani_goruntusu(&[TabloYazimi {
            ad: "bos".into(),
            sql: "CREATE TABLE bos(id INTEGER)".into(),
            satirlar: Vec::new(),
        }]) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(goruntu.len(), 2 * VARSAYILAN_SAYFA_BOYUTU as usize);
    }

    #[test]
    fn satirlar_satir_kimligine_gore_siralanir() {
        let mut tablo = ornek_tablo();
        tablo.satirlar.reverse();
        let sirali = match veritabani_goruntusu_sayfa_boyutu(&[tablo], TEST_SAYFA_BOYUTU) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let siralsiz = match veritabani_goruntusu_sayfa_boyutu(&[ornek_tablo()], TEST_SAYFA_BOYUTU)
        {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(sirali, siralsiz);
    }

    #[test]
    fn cok_satir_icin_coklu_sayfa_uretilir() {
        let mut satirlar = Vec::new();
        for i in 1..=2000i64 {
            satirlar.push((i, vec![Deger::Tam(i), Deger::Metin(format!("ad-{i}"))]));
        }
        let goruntu = match veritabani_goruntusu_sayfa_boyutu(
            &[TabloYazimi {
                ad: "cok".into(),
                sql: "CREATE TABLE cok(id INTEGER, ad TEXT)".into(),
                satirlar,
            }],
            TEST_SAYFA_BOYUTU,
        ) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(goruntu.len() / 512 > 40);
    }

    #[test]
    fn cok_tablo_sema_agacini_kok_1_yapar() {
        let tablolar: Vec<TabloYazimi> = (0..40)
            .map(|i| TabloYazimi {
                ad: format!("tablo_{i}"),
                sql: format!("CREATE TABLE tablo_{i}(id INTEGER, ad TEXT)"),
                satirlar: vec![(1, vec![Deger::Tam(1), Deger::Metin("x".into())])],
            })
            .collect();
        let goruntu = match veritabani_goruntusu_sayfa_boyutu(&tablolar, TEST_SAYFA_BOYUTU) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        // 1. sayfa şema ağacının köküdür: 100. baytta 0x0D (yaprak) veya 0x05 (iç sayfa).
        let kok_turu = goruntu[100];
        assert!(
            kok_turu == 0x0D || kok_turu == 0x05,
            "şema kök sayfa türü 0x{kok_turu:02X}"
        );
    }

    #[test]
    fn dosya_guncelleme_hedefi_degistirir() {
        let dizin = std::env::temp_dir().join(format!("sqlstage-yazici-{}", std::process::id()));
        let _ = fs::create_dir_all(&dizin);
        let yol = dizin.join("g.db");
        let _ = fs::remove_file(&yol);
        if let Err(hata) = dosya_guncelle(&yol, b"ilk") {
            panic!("beklenmeyen hata: {hata}");
        }
        if let Err(hata) = dosya_guncelle(&yol, b"ikinci") {
            panic!("beklenmeyen hata: {hata}");
        }
        let icerik = fs::read(&yol).unwrap_or_default();
        assert_eq!(icerik, b"ikinci");
        let artik: Vec<_> = fs::read_dir(&dizin)
            .map(|girdiler| girdiler.filter_map(Result::ok).collect())
            .unwrap_or_default();
        assert_eq!(artik.len(), 1, "geçici dosya artık kalmamalı");
        let _ = fs::remove_dir_all(&dizin);
    }

    #[test]
    fn hedef_dosya_olmadan_guncelleme_calisir() {
        let dizin = std::env::temp_dir().join(format!("sqlstage-yeni-{}", std::process::id()));
        let _ = fs::create_dir_all(&dizin);
        let yol = dizin.join("yeni.db");
        let _ = fs::remove_file(&yol);
        if let Err(hata) = dosya_guncelle(&yol, b"veri") {
            panic!("beklenmeyen hata: {hata}");
        }
        assert!(yol.exists());
        let _ = fs::remove_dir_all(&dizin);
    }
}
