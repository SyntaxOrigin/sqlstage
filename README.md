# SQLStage (SQLSahne)

Gömülü SQLite dosyalarını **kendi okuyucusuyla** açan, salt okunur bir şema görüntüleyici
ve SQL konsolu. `rusqlite` ya da herhangi bir SQLite C kütüphanesi **kullanılmaz**;
100 baytlık başlık, b-tree sayfaları, varışma zincirleri ve kayıt serileştirmesi elle
uygulanmıştır.

Açılan `.db` dosyasına **tek bir bayt bile yazılmaz**. `UPDATE`, `DELETE`, `DROP`,
`INSERT`, `ATTACH` ve benzeri komutlar açıkça reddedilir.

## Özellikler

- **Kendi SQLite dosya okuyucusu** — 100 baytlık başlık doğrulama, sayfa boyutu (512–65536,
  ikinin kuvveti), metin kodlaması (UTF-8 / UTF-16LE / UTF-16BE), b-tree sayfa gezinme
  (tablo yaprak/iç + indeks yaprak/iç), kayıt (serial type) ayrıştırma, varışma (overflow)
  zincirleri.
- **Salt okunur açılış** — dosya yalnızca okunabilir kipte açılır; sorgu geçmişi bile
  veritabanının yanına değil ayrı bir dosyaya yazılır.
- **Daraltılmış `SELECT` çalıştırıcısı** — yalnızca
  `SELECT <kolonlar> FROM <tek tablo> [WHERE …] [ORDER BY …] [LIMIT n [OFFSET m]]`.
  Filtreler: `=`, `!=`/`<>`, `<`, `>`, `<=`, `>=`, `LIKE` (`%`, `_`), `IS NULL`,
  `IS NOT NULL`, `AND`, `OR`, `NOT`, parantez. Üç değerli (three-valued) mantık uygulanır.
- **Açık reddetme** — `JOIN`, alt sorgu, `UNION`, `GROUP BY`, `HAVING`, `DISTINCT`,
  `IN`, `BETWEEN`, `EXISTS`, `CASE`, `CAST`, fonksiyon çağrıları, tablo takma adı ve
  `tablo.*` sessizce yok sayılmaz; her biri adı ve nedeni belirtilen bir hata üretir.
- **Şema görüntüleyici** — tablo/kolon listesi, sınıflandırılmış tipler (`INTEGER`, `TEXT`,
  `REAL`, `BLOB`, `NUMERIC`, `DATETIME`, `JSON`), nullability, birincil anahtar, yabancı
  anahtar ilişkileri, satır sayısı ve kök sayfa numarası.
- **ASCII şema diyagramı** — tablo kartları ve `kaynak [1] --- [N] hedef (kolon)` ilişki
  satırları.
- **Sorgu planı açıklaması** — `EXPLAIN QUERY PLAN` yerine, dolaşılacak b-tree sayfası ve
  okunacak bayt tahmini; her çıktı "ölçülmedi" uyarısı taşır.
- **Terminal sonuç ızgarası** — kolon sığmazsa `~` ile kısaltma, `NULL` ve blob gösterimi.
- **JSON dışa aktarma** — `NULL`, tamsayı, gerçek, metin ve blob tipini koruyan belge.
- **Örnek veri üretimi** — `sample` (üç tablolu, 28 satırlık, deterministik küçük e-ticaret
  veri seti), `create` (yeni tablo), `insert` (satır ekle).
- **Sorgu geçmişi** — her çalıştırılan sorgu `sorgular.jsonl` dosyasına bir satır olarak
  eklenir; `--gecmis-no N` ile tekrar çalıştırılabilir.
- **Bütünlük denetimi** — başlık, hücre işaretçisi, varışma zinciri ve ağaç döngüsü
  denetimi (`--butunluk`).

## Kurulum

Gereksinim: Rust **1.74** veya üzeri (bu makinede `rustc 1.98.1` ile derlendi).

```bash
cargo build --release
```

```bash
cargo install --path .
```

## Kullanım

Aşağıdaki çıktıların tamamı bu depodaki ikili ile üretilmiştir. Windows PowerShell'de
`$env:SQLSTAGE_HOME` bir geçici dizine ayarlanmıştır; sorgu geçmişi buraya yazılır.
(`schema` çıktısındaki sütun tablolarının satır sonu boşlukları okunabilirlik için
kaldırılmıştır; başka hiçbir yer değiştirilmemiştir.)

### 1. Örnek veri tabanı üretme

```bash
$ sqlstage sample market.db
market.db yazıldı: 3 tablo (urunler, musteriler, siparisler), toplam 28 satır. Kategoriler: elektronik, ev, kitap, giysi, spor.
```

```bash
$ sqlstage tables market.db
Tablo listesi:
  urunler                    6 kolon       8 satir  kok sayfa 2
  musteriler                 5 kolon       8 satir  kok sayfa 3
  siparisler                 7 kolon      12 satir  kok sayfa 4
```

### 2. Şema ve ASCII diyagram

```bash
$ sqlstage schema market.db --butunluk
market.db · 4 sayfa · 4096 bayt/başlık · kodlama Utf8 · WAL kapalı

urunler  (8 satır, kök sayfa 2, 1 sayfa)
  KOLON                    TİP          NULL   PK       VARSAYILAN
  id                       INTEGER      evet   evet
  ad                       TEXT         hayır  -
  kategori                 TEXT         evet   -
  fiyat                    REAL         evet   -
  stok                     INTEGER      evet   -
  etiket                   BLOB         evet   -
  birincil anahtar: id

musteriler  (8 satır, kök sayfa 3, 1 sayfa)
  KOLON                    TİP          NULL   PK       VARSAYILAN
  id                       INTEGER      evet   evet
  ad                       TEXT         hayır  -
  sehir                    TEXT         evet   -
  yas                      INTEGER      evet   -
  kayit_tarihi             TEXT         evet   -
  birincil anahtar: id

siparisler  (12 satır, kök sayfa 4, 1 sayfa)
  KOLON                    TİP          NULL   PK       VARSAYILAN
  id                       INTEGER      evet   evet
  musteri_id               INTEGER      hayır  -
  urun_id                  INTEGER      hayır  -
  adet                     INTEGER      evet   -
  tutar                    REAL         evet   -
  durum                    TEXT         evet   -
  tarih                    TEXT         evet   -
  birincil anahtar: id

Bütünlük denetimi: 4 sayfa, 0 sorun
```

```bash
$ sqlstage schema market.db --diyagram
== Tablolar ==
+-----------------+-----------------
|    urunler     |
+-----------------+-----------------
| id INTEGER PK    |
| ad TEXT NN       |
| kategori TEXT    |
| fiyat REAL       |
| stok INTEGER     |
| etiket BLOB      |
+-----------------+-----------------
+---------------------+---------------------
|     musteriler     |
+---------------------+---------------------
| id INTEGER PK        |
| ad TEXT NN           |
| sehir TEXT           |
| yas INTEGER          |
| kayit_tarihi TEXT    |
+---------------------+---------------------
+-------------------------+-------------------------
|       siparisler       |
+-------------------------+-------------------------
| id INTEGER PK            |
| musteri_id INTEGER NN    |
| urun_id INTEGER NN       |
| adet INTEGER             |
| tutar REAL               |
| durum TEXT               |
| tarih TEXT               |
+-------------------------+-------------------------

== Iliskiler ==
musteriler [1] --- [N] siparisler (id)  (musteri_id)
urunler [1] --- [N] siparisler (id)  (urun_id)
```

### 3. Sorgu çalıştırma

```bash
$ sqlstage query market.db "SELECT id, ad, sehir, yas FROM musteriler WHERE sehir = 'Izmir' OR yas IS NULL ORDER BY ad"
+--------------+------------+--------------+---------------+
| id (INTEGER) | ad (TEXT)  | sehir (TEXT) | yas (INTEGER) |
+--------------+------------+--------------+---------------+
| 1            | Ali Yilmaz | Izmir        | 21            |
| 6            | Emre Koc   | Izmir        | NULL          |
+--------------+------------+--------------+---------------+
2 satır (taranan 8, eşleşen 2), 0 ms
```

```bash
$ sqlstage query market.db "SELECT * FROM siparisler WHERE durum = 'kargoda' ORDER BY tutar DESC LIMIT 3" --plan
Sorgu planı (tahmin; sorgu çalıştırılmadan ÖLÇÜLMEDİ)
  sorgu          : SELECT * FROM siparisler WHERE durum = 'kargoda' ORDER BY tutar DESC LIMIT 3
  erişim         : tam tablo taraması (TABLO TARAMA)
  tablo          : siparisler
  kök sayfa      : 4
  sayfa boyutu   : 4096 bayt
  dolaşılan sayfa: 1 (yapısal sayma ile ölçüldü)
  okuma tahmini  : ~4096 bayt (4 KiB)
  hücre yükü     : ~434 bayt
  filtre         : 1 karşılaştırma (indeks kullanılmaz)
  sıralama       : tam satır sıralaması gerekir
  dilimleme      : LIMIT/OFFSET sonuca uygulanır; taranan satır sayısı azalmaz
  uyarı          : bu bir sorgu iyileştiricisi değildir; SQLite'in EXPLAIN QUERY PLAN çıktısıyla karşılaştırılamaz.

+--------------+----------------------+-------------------+----------------+--------------+--------------+--------------+
| id (INTEGER) | musteri_id (INTEGER) | urun_id (INTEGER) | adet (INTEGER) | tutar (REAL) | durum (TEXT) | tarih (TEXT) |
+--------------+----------------------+-------------------+----------------+--------------+--------------+--------------+
| 2            | 1                    | 6                 | 4              | 19960.0      | kargoda      | 2025-02-02   |
| 10           | 1                    | 6                 | 4              | 19960.0      | kargoda      | 2025-10-10   |
| 6            | 5                    | 2                 | 4              | 878.0        | kargoda      | 2025-06-06   |
+--------------+----------------------+-------------------+----------------+--------------+--------------+--------------+
3 satır (taranan 12, eşleşen 3), 0 ms
```

### 4. JSON dışa aktarma

```bash
$ sqlstage export market.db "SELECT ad, fiyat FROM urunler WHERE kategori = 'elektronik' ORDER BY fiyat" --cikti sonuc.json
sonuc.json yazıldı (3 satır)
```

```json
{
  "surum": 1,
  "uretici": "SQLStage 0.1.0",
  "sorgu": "SELECT ad, fiyat FROM urunler WHERE kategori = 'elektronik' ORDER BY fiyat",
  "sutunlar": ["ad", "fiyat"],
  "sutun_tipleri": ["TEXT", "REAL"],
  "satir_sayisi": 3,
  "taranan_satir": 8,
  "eslesen_satir": 3,
  "sure_ns": 139400,
  "satirlar": [
    ["Fare", 219.5],
    ["Klavye", 449.9],
    ["Kulaklik", 899.99]
  ]
}
```

(`serde_json` girintilmiş çıktı üretir; yukarıdaki `satirlar` dizisi kısaltılmıştır.)

### 5. Tablo oluşturma, satır ekleme ve geçmiş

```bash
$ sqlstage create notlar.db --tablo notlar --kolon "id:INTEGER:pk" --kolon "baslik:TEXT:notnull" --kolon "icerik:TEXT" --kolon "oncelik:INTEGER"
notlar.db yazıldı: tablo `notlar` (4 kolon)
$ sqlstage insert notlar.db --tablo notlar --deger "baslik=Alis listesi" --deger "icerik=Sabah toplantisi icin hazirlandi" --deger "oncelik=1"
`notlar` tablosuna 1 satır eklendi (satır kimliği 1), toplam 1
$ sqlstage query notlar.db "SELECT id, baslik, oncelik FROM notlar WHERE oncelik <= 2 ORDER BY oncelik"
+--------------+---------------+-------------------+
| id (INTEGER) | baslik (TEXT) | oncelik (INTEGER) |
+--------------+---------------+-------------------+
| 1            | Alis listesi  | 1                 |
+--------------+---------------+-------------------+
1 satır (taranan 1, eşleşen 1), 0 ms
```

Sorgu geçmişine yazılan satır (`sorgular.jsonl`, tek satır):

```json
{"zaman":1790703233,"veritabani":"notlar.db","sorgu":"SELECT id, baslik, oncelik FROM notlar WHERE oncelik <= 2 ORDER BY oncelik","sure_ns":64000,"satir_sayisi":1,"hata":""}
```

```bash
$ sqlstage query notlar.db --gecmis-no 1 --gecmis-yazma
+--------------+---------------+-------------------+
| id (INTEGER) | baslik (TEXT) | oncelik (INTEGER) |
+--------------+---------------+-------------------+
| 1            | Alis listesi  | 1                 |
+--------------+---------------+-------------------+
1 satır (taranan 1, eşleşen 1), 0 ms
```

### 6. Yazma komutlarının reddi

```bash
$ sqlstage query notlar.db "UPDATE notlar SET baslik = 'X'"
hata: yazma komutu reddedildi: "UPDATE" — SQLStage veritabanı dosyasını salt okunur açar
$ echo $?
1
$ sqlstage query notlar.db "SELECT * FROM notlar JOIN musteriler ON notlar.id = musteriler.id"
hata: desteklenmeyen SQL yapısı: JOIN — birleştirme desteklenmiyor
```

## Test

```bash
cargo test
```

Gerçek çıktı:

```
running 181 tests
test result: ok. 181 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**test sonucu: okunan 202; başarısız 0** (181 birim + 11 entegrasyon + 5 gerçek SQLite
uyumluluğu + 5 güvenlik/salt okunurluk).

Kapsanan kenar durumları:

| Alan | Örnek testler |
|---|---|
| Bozuk başlık | Yanlış sihirli sabit, 100 bayttan kısa dosya, sayfa boyutu 0/3/513/1000/65535, sayfa boyutu 1 → 65536, kodlama 0/4/5/1000, rezerve alan, yazma sürümü 7, şema biçimi 0, sayfa sayısı dosyadan büyük |
| Bozuk b-tree | Tanınmayan sayfa türü, hücre işaretçisi sayfa dışında, aşırı hücre sayısı, kesik varint, indeks sayfasının tablo ağacında bulunması, ağaç döngüsü, derinlik sınırı |
| Varışma (overflow) | Çok büyük metin kaydının (120 000 bayt) yazılıp okunması, kopuk zincir, döngü, 0 sayfasına devam, kısa varışma sayfası |
| Boş tablo | 0 satırlı tablo: 1 yaprak sayfa, 0 sonuç satırı |
| Tüm SQL tipleri | NULL, 1/2/3/4/6/8 bayt tamsayılar, IEEE-754 gerçek, metin, boş metin, blob, boş blob; `kayit_yaz` ↔ `kayit_ayikla` gidiş-dönüşü |
| Boş veritabanı | 0 tablolu dosya: "(şemada tablo yok)" |
| Şema gidiş-dönüşü | `create` ile üretilen tablonun şeması yeniden okunduğunda kolon/tip/PK bilgisi aynı |
| Filtre operatörleri | `=`, `!=`, `<`, `>`, `<=`, `>=`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`, parantez |
| LIKE jokerleri | `%`, `_`, önde/sonra/ortada `%`, art arda `%`, eşleşmeme, ASCII büyük/küçük harf duyarsızlığı, metin dışı değerde `NULL` |
| NULL tuzağı | `= NULL` ayrıştırıcı tarafından `IS NULL` yönlendirmesiyle reddedilir; `IS NULL` doğru sonuç verir; üç değerli `AND`/`OR` kısa devreleri |
| LIMIT / ORDER BY | `LIMIT`, `LIMIT … OFFSET`, `ASC`/`DESC`, NULL sıralaması, takma ad (`AS`) |
| Desteklenmeyen SQL | `JOIN` (4 çeşit), `UNION`, `GROUP BY`, `HAVING`, `WITH`, alt sorgu, `COUNT(*)`, `IN`, `BETWEEN`, `EXISTS`, tablo takma adı, `tablo.*` |
| Yazma komutu reddi | `UPDATE`, `DELETE`, `DROP`, `INSERT`, `CREATE`, `ALTER`, `ATTACH`, `DETACH`, `PRAGMA`, `VACUUM`, `REPLACE`, `BEGIN`, `TRUNCATE` — hepsi `YazmaReddi` |
| Izgara sığmama | 300 karakterlik hücre `--genislik 12` ile kısaltılır, tüm satırlar aynı genişlikte kalır |
| JSON dışa aktarma | Sütun adları/tipleri, satır sayısı, `null`, blob `{"$blob":"…"}`, boş sonuç kümesi |
| Sayfa bütünlüğü | Sağlam dosyada 0 sorun; bozuk sayfa türünde sorun yakalanır; döngü ve indeks sayfası tespiti |
| Gerçek SQLite dosyası | SQLite 3.41'in ürettiği 512 baytlık sayfalı, **indeksli** veritabanı: başlık, şema, satırlar, sorgular, bütünlük |
| UTF-16 veritabanı | UTF-16LE kodlamalı gerçek dosya okunur, Türkçe karakterler bozulmaz |
| Salt okunurluk | Şema + bütünlük + plan + çok sayıda sorgudan sonra dosya **bayt bayt aynıdır**; salt okunur işaretli dosya açılıp okunur; reddedilen komut dosyayı değiştirmez |
| Sorgu geçmişi | JSONL satırı yazılır, okunur, numarayla tekrar çalıştırılır, bozuk satırlar atlanır |

Diğer kalite komutları:

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Her ikisi de çıktı üretmeden temiz geçer.

## Proje Yapısı

```
27-sqlstage/
├── Cargo.toml
├── Cargo.lock
├── LICENSE.txt
├── README.md
├── .gitignore
├── src/
│   ├── lib.rs              çekirdek kütüphane + modül haritası
│   ├── main.rs             clap CLI kabuğu (7 alt komut)
│   ├── hata.rs             SahneHata enum + Display/Error
│   ├── deger.rs            NULL / INTEGER / REAL / TEXT / BLOB + sıralama
│   ├── varint.rs           varint okuma + yazma
│   ├── baslik.rs           100 baytlık başlık ayrıştırma ve doğrulama
│   ├── sayfa.rs            b-tree sayfa/hücre ayrıştırma, yerel yük hesabı
│   ├── kayit.rs            serial type ayrıştırma, varışma birleştirme
│   ├── veritabani.rs       salt okunur dosya erişimi, tablo taraması, bütünlük
│   ├── sema.rs             şema modeli + CREATE TABLE alt kümesi ayrıştırıcısı
│   ├── sql/
│   │   ├── mod.rs
│   │   ├── lexer.rs        sözcük tarağıcısı
│   │   ├── ast.rs          sorgu soyut sözdizim ağacı
│   │   ├── parser.rs       ayrıştırıcı + desteklenmeyen yapı reddi
│   │   ├── filtre.rs       üç değerli mantık, LIKE, karşılaştırma
│   │   └── yurutucu.rs     tarama → filtre → sıralama → dilimleme
│   ├── izgara.rs           terminal sonuç ızgarası (kısaltmalı)
│   ├── json_cikti.rs       JSON dışa aktarma belgesi
│   ├── diyagram.rs         ASCII şema diyagramı ve ilişki satırları
│   ├── plan.rs             sorgu planı + maliyet tahmini
│   ├── ornek.rs            deterministik örnek veri seti
│   ├── gecmis.rs           sorgular.jsonl geçmişi
│   └── yazici.rs           minimal SQLite dosya yazıcısı (örnek veri için)
└── tests/
    ├── yardimci/mod.rs     geçici dizin yardımcısı + gerçek SQLite fixture'ları
    ├── entegrasyon.rs      uçtan uca akışlar
    ├── gercek_sqlite.rs    üçüncü taraf dosya uyumluluğu
    └── salt_okunur.rs      salt okunurluk ve yazma reddi kanıtları
```

Satır sayıları: kaynak kod 7 918 satır, test kodu 903 satır (toplam 8 821).

## Yapılandırma

| Ayar | Tür | Varsayılan | Etkisi |
|---|---|---|---|
| `SQLSTAGE_HOME` | ortam değişkeni | yürütülebilinin bulunduğu dizin | Sorgu geçmişi `sorgular.jsonl` dosyasının yeri. Salt okunur bir USB'de çalıştırırken yazılabilir bir dizin seçilmelidir. |
| `--genislik <n>` | `query` bayrağı | `28` | Sonuç ızgarasında kolon başına en fazla karakter; aşan hücreler `~` ile kısaltılır. |
| `--json` | `query` bayrağı | yok | Sonucu ızgara yerine JSON olarak basar. |
| `--plan` | `query` bayrağı | yok | Sorguyu çalıştırmadan önce plan açıklaması ve maliyet tahmini basar. |
| `--gecmis-dosya <yol>` | `query` bayrağı | `SQLSTAGE_HOME/sorgular.jsonl` | Geçmiş dosyasının konumunu geçici olarak değiştirir. |
| `--gecmis-no <n>` | `query` bayrağı | yok | Geçmişteki 1 tabanlı numaralı sorguyu yeniden çalıştırır. |
| `--gecmis-yazma` | `query` bayrağı | yok | Sorguyu geçmişe **yazmaz** (sonsuz döngüyü önler). |
| `--cikti <yol>` | `export` bayrağı | yok | JSON'u dosyaya yazar; verilmezse standart çıktıya basar. |
| `--zorla` | `sample`, `create` bayrağı | yok | Var olan dosyanın üzerine yazmayı kabul eder. |
| `--diyagram` | `schema` bayrağı | yok | Kolon listesi yerine ASCII diyagramı basar. |
| `--butunluk` | `schema` bayrağı | yok | Sayfa bütünlüğü denetimini çalıştırır ve raporlar. |
| `SQLSTAGE_PAGE_SIZE` | — | yok | **Yoktur.** Yazıcının sayfa boyutu sabittir (4096 bayt; testlerde 512). Okuyucu her sayfa boyutunu kabul eder. |

Yapılandırma dosyası (`config/`, `settings.json`) yoktur: tüm ayarlar bayrak veya tek bir
ortam değişkenidir.

## Bilinen Sınırlamalar

**MANIFEST kartından taşınan ertelenenler:**

- **Şema görselleştirme (kutu-ok diyagramı)** yerine ASCII tablo kartları + ilişki satırları
  üretilir. Tam kutu-ok yerleşimi ve kenar çizimi yoktur.
- **Gerçek `EXPLAIN QUERY PLAN`** yoktur; yerine kendi maliyet tahminimiz vardır ve bu
  tahmin SQLite'in sorgu iyileştiricisiyle karşılaştırılamaz.
- **Not + grafik dışa aktarma (HTML/Markdown)** yoktur. `export` yalnızca JSON üretir;
  grafik üretimi ve gömülü HTML çıktısı yapılmamıştır.
- **Sorgu geçmişi için adlandırılmış/klasörlenmiş kayıtlı sorgular** yoktur; geçmiş
  düz bir JSONL dosyasıdır ve numarayla çağrılır.
- **SQL editörü / etkileşimli konsol** yoktur; tüm komutlar tek seferlik CLI çağrılarıdır.

**Kendi okuyucu kaynaklı sınırlamalar:**

- **`WITHOUT ROWID` tablolar** desteklenmez. Bu tablolar indeks b-tree kullanır ve şema
  ayrıştırıcısı açık bir hata verir.
- **`STRICT` tablolar** desteklenmez (tip zorlaması uygulanmaz).
- **WAL kurtarma yapılmaz.** WAL modunda `-wal` dosyasındaki son commit edilmemiş
  sayfalar görünmez; `schema` çıktısında `WAL etkin` uyarısı verilir. Salt okunurluk
  ilkesi gereği `-wal` ve `-shm` dosyalarına dokunulmaz.
- **Şifreli veritabanları** (SQLCipher vb.) desteklenmez; başlık doğrulaması zaten
  reddedilir.
- **Boş liste (freelist) sayfaları** yalnızca sessizce atlanır; `integrity_check`'in
  aksine ayrıntılı bir boş liste denetimi yapılmaz.
- **Gizli (freelist/pointer-map) sayfalar** bütünlük denetimine girmez.
- **Hücre başlıklarında bildirilen boyut alanları güvenilmez kabul edilir ve sınır
  dışıysa reddedilir.** `varint(yük)` alanı dosyadan gelen ham bir sayıdır; üst sınırı
  olmadığı için **ne bellek ön-tahsisi için kullanılır ne de okuma sınırı sayılır**.
  Varışma zinciriyle karşılanamayan boyutlar (dosyada karşılığı olmayan `i64::MAX`,
  `2^62` gibi değerler) kontrollü bir `bozuk varışma zinciri` hatasıyla reddedilir —
  araç bellek tüketip çökmez. Negatif boyut alanları sıfıra çevrilir ve kayıt ayrıştırma
  aşamasında geçersiz başlık hatasına düşer. Yük, varışma sayfaları geldikçe blok blok
  büyütülen bir tampona yazılır; her sayfa en fazla bir kez ziyaret edilebilir (döngü
  koruması), dolayısıyla bellek tüketimi dosya boyutuyla sınırlıdır.
- **Metin harmanlaması (collation)** yalnızca `BINARY`'dir. `NOCASE`, `RTRIM` ve
  `COLLATE` ifadeleri desteklenmez.
- **`LIKE`** ASCII büyük/küçük harf duyarsızdır ve **kaçış karakteri yoktur** (`%` ve `_`
   joker olarak yorumlanır). SQLite'in `ESCAPE` yan cümlesi desteklenmez.
- **Sorgu sonuçları bellekte tutulur** (vektör). Yüz binlerce satırlık sonuçlarda bellek
  kullanımı satır sayısıyla doğru orantılıdır; raporun "sanat kaydırmalı ızgara" hedefi
  uygulanmamıştır.
- **Dosyanın tamamı belleğe alınır.** Çok büyük veritabanlarında bellek tüketimi dosya
  boyutuyla orantılıdır (hedef: 180 MB tepe RSS).
- **Sorgu geçmişi dosyası program dizinine yazılabilir.** Salt okunur bir ortamda
  `SQLSTAGE_HOME` ayarlanmazsa geçmiş yazımı başarısız olur ve bu durum kullanıcıya
  `uyarı:` ile bildirilir; sorgu yine de çalışır.
- **`create` ve `insert` yalnızca sınırlı dosyalarda çalışır.** Dosya indeks, `WITHOUT
  ROWID` tablosu veya okunamayan bir yapı içeriyorsa **yazma başlamadan reddedilir** ve
  dosya olduğu gibi bırakılır. Yeniden yazma tam dosya yeniden üretimi yapar; silinmiş
  (freelist) sayfalar geri kazanılmaz ve dosya şişebilir.
- **Yazıcının ürettiği `INTEGER PRIMARY KEY` kolonu** kayıtta `NULL` yerine verilen
  değerle yazılır (okuyucu okurken zaten satır kimliğini kullanır, bu yüzden sonuçlar
  doğrudur; yalnızca bayt düzeyindeki temsil SQLite'inkinden farklı olabilir).
- **`clap` yardımcı metinleri İngilizcedir.** `clap` 4'ün yerleşik metinleri yerelleştirilemez;
  kendi yardım metinlerimiz Türkçedir.
- **Tek `#[allow]` kullanımı:** `tests/salt_okunur.rs` içinde
  `#[allow(clippy::permissions_set_readonly_false)]` — yalnızca test izini temizlemek için
  kullanılır; üretim kodunda hiçbir `#[allow]` yoktur.
- **`unwrap` / `expect` / `panic!` üretim kodunda yoktur**; yalnızca `#[cfg(test)]`
  modüllerinde, test verisi hazırlarken kullanılır.

## Gelecek Geliştirmeler

1. Tam kutu-ok ASCII şema diyagramı (kenar kesişimi önleyen basit yerleşim).
2. Markdown + gömülü HTML not/grafik dışa aktarımı.
3. Adlandırılmış ve klasörlenmiş kayıtlı sorgular (`--kayitli <ad>`).
4. B-tree **iç sayfa** indekslerini okuyup `WHERE` koşulunu indeks aramasına yönlendirmek
   (basit bir "indeks kullanılabilir mi" tahmini).
5. `NULLS FIRST/LAST`, `COLLATE NOCASE` ve `ESCAPE` desteği.
6. Sonuç ızgarasında sayfalanmış/lazy yükleme ile bellekten bağımsız büyük sonuçlar.
7. `WITHOUT ROWID` tablolar için indeks b-tree okuma desteği.
8. **Varışma yükleri için güvenli ön-tahsis.** Bugün yük tamponu ön-tahsis olmadan,
   her varışma sayfası geldikçe blok blok büyütülür. Okuyucu dosya boyutunu bildiği
   için `min(bildirilen_yuk, dosya_boyutu - sayfa_ofseti)` gibi bir tavanla sınırlı
   ön-tahsis (örneğin 64 KiB'lık dilimler halinde `reserve`) büyük kayıtlarda
   yeniden kopyalamayı azaltır. Güvenlik gereği bu tavan **dosya boyutundan büyük
   olamaz**; aksi hâlde `i64::MAX` yazan tek bir hücre yeniden `abort` yolunu açar.

## Troubleshooting

| Belirti | Neden | Çözüm |
|---|---|---|
| `geçersiz veritabanı: başlık sihirli sabiti "SQLite format 3\0" değil` | Dosya bir SQLite veritabanı değil (örneğin CSV, eski `.db-journal`, ya da SQLite olmayan bir ikili). | Dosyanın gerçekten SQLite veritabanı olduğunu doğrulayın; `file`/`sqlite3` ile bakabilirsiniz. |
| `geçersiz başlık alanı sayfa_boyutu=…` veya `kodlama=… desteklenmiyor` | Dosya bozuk ya da kısmi kopyalanmış; başlıkta geçersiz değer var. | Dosyayı yeniden kopyalayın. SQLStage bozuk başlığı "onararak" devam etmez. |
| `WAL etkin` uyarısı ve eski satırlar | Veritabanı WAL modunda; son commit edilmemiş sayfalar `-wal` dosyasındadır ve SQLStage onu açmaz. | Veritabanını checkpoint edin (`sqlite3 veritabani.db "PRAGMA wal_checkpoint(TRUNCATE);"`) veya WAL'i kapatın. |
| `tablo bulunamadı: "x" (mevcut: …)` | Tablo adı büyük/küçük harf ya da tırnak farkı içeriyor, ya da dosyada gerçekten yok. | `sqlstage tables <dosya>` ile mevcut adları görün; SQL'de tırnaklı kimlik kullanın. |
| `desteklenmeyen SQL yapısı: JOIN — …` | Desteklenen alt küme tek tabloyla sınırlıdır. | Sorguyu iki ayrı `query` çağrısına bölün ve sonuçları karşılaştırın; ya da SQLite kullanan bir istemciye geçin. |
| `yazma komutu reddedildi: "UPDATE"` | SQLStage veritabanını salt okunur açar; bu bir hata değil, güvenlik politikasıdır. | Değişiklik yapmak için `insert`/`create` komutlarını kullanın (yalnızca kendi oluşturduğunuz dosyalarda). |
| `dosya zaten var; üzerine yazmak için --zorla verin` | Var olan dosya korunuyor. | `--zorla` ekleyin ya da başka bir dosya adı seçin. |
| `indeks içeren veritabanları yeniden yazılamaz` | `insert`, dosyayı baştan yazar; indeksleri koruyamaz. | `insert` kullanmayın; veriyi Python/`sqlite3` gibi bir araçla ekleyin. |
| `uyarı: sorgu geçmişine yazılamadı: …` | `SQLSTAGE_HOME` yazılabilir değil (örneğin salt okunur USB). | `SQLSTAGE_HOME` ayarlayın veya `--gecmis-yazma` kullanın. |
| Izgarada değerler `…` ile kesiliyor | Kolon genişliği sınırı aşıldı. | `--genislik 60` ya da `--json` kullanın. |

## Atıflar

- SQLite dosya biçimi belgesi — <https://www.sqlite.org/fileformat2.html>
- SQLite dil başvurusu (SQL alt kümesi) — <https://www.sqlite.org/lang.html>
- SQLite `LIKE` işleci — <https://www.sqlite.org/lang_expr.html>
- SQLite telif/kamu malı beyanı — <https://www.sqlite.org/copyright.html>
- SQLite `EXPLAIN QUERY PLAN` — <https://www.sqlite.org/eqp.html>
- `clap` (CLI çerçevesi) — <https://docs.rs/clap/>
- `serde` / `serde_json` — <https://serde.rs/> · <https://github.com/serde-rs/json>
- Rust standart kütüphane belgeleri — <https://doc.rust-lang.org/std/>
- Rust 2021 edition rehberi — <https://doc.rust-lang.org/edition-guide/edition-2021/>
- SQLite fiyat dökümü (raporda kaynak olarak geçen aracın davranış referansı) —
  <https://sqlite-utils.datasette.io/>
- Rapor dosyasının kendisi: `%USERPROFILE%\Desktop\Fikirler\27-sql-sahne-oyun-alani.html`
  (iç tasarımın kaynağı; yerel yol olarak verilmiştir, URL değildir).
- Test fixture'ları SQLite 3.41'in Python `sqlite3` modülüyle üretilmiştir
  (`PRAGMA page_size=512`). Python yalnızca **test verisi üretmek** için kullanılmıştır;
  proje kodu olarak hiçbir bağımlılığı yoktur.

## Lisans

MIT. Kaynak kod lisansı için bkz. [`LICENSE.txt`](LICENSE.txt).

SQLite kamu malıdır (public domain); bu proje SQLite kodunu **içermez**, yalnızca onun
yayımlanmış dosya biçim belgesini uygular. Bkz. <https://www.sqlite.org/copyright.html>.
