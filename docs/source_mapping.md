# Varlık → Kaynak Eşlemesi (Source Mapping)

Bu doküman, `MarketData` içinde "varlık kodu" verildiğinde hangi kaynaktan veri alınacağını ve varlık türünün (`asset_type`) nasıl tespit edileceğini tarif eder. Amaç hatalı kaynak seçimini engellemek, otomatik ve güvenli seçim sağlamak, ve gerektiğinde kullanıcıya geçersiz kılma imkânı sunmaktır.

## TL;DR
- Kodda güvenli varsayılan eşlemeler olacak.
- Runtime olarak `MarketData/config/source_mapping.yaml` ile override edilebilecek.
- Seçim iki seviyede zorlanacak: CLI/bridge ve Hub/Etl.
- Öncelik: 1) sembol kayıt defteri (symbol registry) 2) desen/heuristik 3) kullanıcı onayı/force-source.

## Hedefler
1. Varlık türü hatalarını engelle (örn. hisse için `tefas` kullanımı).
2. Otomatik seçim ve fallback zinciri uygula.
3. CLI ve programatik çağrılarda tutarlı davranış.
4. Kolay güncellenebilir konfigürasyon ile saha operasyonu.

## Veri modeli (konfig şeması)

Konfig (örnek): `MarketData/config/source_mapping.yaml`

```yaml
source_mapping:
  defaults:
    crypto:
      kline: ["binance", "btcturk"]
      tick: ["binance", "btcturk"]
    equities:
      kline: ["yahoo", "kap"]
      fundamentals: ["yahoo"]
    funds:
      fundamentals: ["tefas"]
    forex:
      tick: ["dovizcom"]

  symbol_overrides:
    BTCUSDT:
      type: crypto
      preferred_sources: ["binance"]
    AAPL:
      type: equities
      preferred_sources: ["yahoo"]
    TRFUND001:
      type: funds
      preferred_sources: ["tefas"]
```

`defaults` genel politikayı, `symbol_overrides` bilinen semboller için kesin kuralları tutar.

## Algoritma: varlık türü tespiti ve kaynak seçimi (yüksek seviyede)

1. Normalizasyon: giren sembolü büyük/küçük harf, whitespace ve Unicode normalize ederek temizle.
2. Symbol registry kontrolü: `symbol_overrides` veya ayrı bir `symbols.csv`/DB sorgulanır. Eğer eşleşme varsa `type` ve `preferred_sources` kullanılır.
3. Heuristikler (registry yoksa):
   - ISIN kontrolü: `^[A-Z]{2}[A-Z0-9]{10}$` → muhtemelen `equities`/`funds`/`bonds` (buna göre ilave metadata sorgula).
   - Crypto suffix: sembol `USDT`, `BTC`, `ETH`, `BUSD`, `SOL`, `ADA` gibi eklerle bitiyorsa → `crypto`.
   - Exchange-suffixed tickers: `AAPL.US`, `BABA.HK` gibi pattern varsa → `equities` ve exchange bilgisi çıkarılır.
   - Numerik ya da TC/yerel fon kodu desenleri (örnek: TRFUND...) → `funds`.
   - Parite desenleri `USDTRY`, `EURUSD` → `forex`.
4. Dataset bağlamı: çağrılan dataset (`kline`, `fundamentals`, `tick`) seçimi daraltır. Örn. `fundamentals` isteyen çağrı `fund`/`equities` olmalı; `tick` genellikle `crypto`/`forex`/`exchange` odaklı.
5. Eşleme: tespit edilen `asset_type` ve istenen `dataset` için `defaults` içinden tercihli kaynak listesini al.
6. Registry doğrulama: seçilecek kaynak `SourceAdapterRegistry` içinde kayıtlı mı? Değilse sıradaki yedeğe geç.
7. Son çare: eğer hiçbir uygun kaynak yoksa hata döndür veya `offline` fallback uygula (konfig ile belirlenir).

## Örnek karar akışı

Girdi: sembol=`BTCUSDT`, dataset=`kline` →
- Registry bulunduysa `type=crypto`, preferred_sources `binance` → `binance` mevcutsa seçilir.
- Değilse suffix `USDT` heuristiği crypto döndürür → defaults.crypto.kline listesinden ilk mevcut adaptör seçilir.

Girdi: sembol=`AAPL`, dataset=`fundamentals` →
- Registry olmalı (AAPL büyük bir sembol, ISIN/metadata ile teyit tercih edilir). `fundamentals` ise `equities` kaynakları (`yahoo`) seçilir. `tefas` kullanılmaz.

Girdi: sembol=`TRFUND001`, dataset=`fundamentals` →
- `symbol_overrides` veya fon kodu desenleri ile `funds` tespit edilir → `tefas` önerilir.

## CLI davranışı (`market_data_bridge` önerisi)

- `--source` verilirse: önce `SourceSelector` ile uyumluluk denetlenir; uyumsuzsa davranış konfige göre:
  - varsayılan: otomatik uygun kaynağı seç ve uyarı ver (veya hata döndürme seçeneği)
  - `--force-source <id>` ile doğrudan geçersiz kılma mümkün
- `--dry-run` veya `--explain-source` ile hangi kaynağın seçileceğini ve nedenini göster

Örnek komut:
```bash
market_data_bridge ingest --symbol AAPL --datasets fundamentals --explain-source
# Çıktı: symbol=AAPL -> detected_type=equities -> chosen_source=yahoo (preferred)
```

## Doğrulama & Test Planı
- Birim testler: `tests/source_mapping_tests.rs` — registry override, heuristics, fallback, force-source.
- CLI entegrasyon testleri: `bridge_cli_tests.rs` içine 2 senaryo ekle (otomatik seçim, force override).
- CI: testi çalıştırırken örnek `source_mapping.yaml` yüklenmiş olsun.

## Operasyonel notlar
- Kök neden: sembol isimlendirme evrensel değil; en sağlam çözüm bir sembol kaydı (registry) tutmaktır. Heuristikler sadece bir backstop olmalı.
- Sembol kaydını (paralel) beslemek için başlangıçta açık kaynak exchange listeleri, ISIN listeleri veya organizasyonel CSV kullanılabilir.
- Audit: otomatik seçimlerin logfile'a kaydedilmesi (hangi kural seçti, hangi kaynak kullanıldı).

## Next steps (kısa)
1. Bu dokümanı kabul ederseniz `MarketData/src/source_mapping.rs` için iskeleti oluştururum.
2. Ardından CLI entegrasyonunu ve `DataHub` doğrulamasını ekleyip testleri yazarım.

---
Doküman kaydedildi: `MarketData/docs/source_mapping.md`

## Detailed Implementation Plan

Bu bölüm, planın adım adım uygulanması için somut iş paketlerini, kabul kriterlerini, test stratejisini, CI entegrasyonunu ve tahmini zaman çizelgesini içerir. Uzun bir iş olabileceği için işleri küçük, doğrulanabilir parçalara böldük.

Milestones:
- M1 — Altyapı ve Registry: `SourceMetadata` yapısı, `SourceRegistry` loader ve metadata konfigürasyonu.
- M2 — Detection Core: regex/heuristic kuralları, candidate generator, scoring mekanizması.
- M3 — Selector & API: `SourceSelector` implementasyonu ve `select(symbol, dataset, opts)` API'si.
- M4 — Entegrasyon: CLI (`market_data_bridge`) ve `DataHub`/`Etl`'e zorlayıcı entegrasyon.
- M5 — Sağlık + Operasyon: kaynak health checks, caching, explain/logging ve CI testleri.

Work items (yüksek seviye, her birinin kabul kriterleriyle):

1) `SourceMetadata` ve `SourceRegistry` (M1)
- Dosyalar: `MarketData/src/source_registry.rs`, `MarketData/config/source_metadata.yaml`
- Açıklama: Her kaynak için `id`, `supported_asset_classes`, `supported_datasets`, `api_templates`, `priority` ve opsiyonel `health_probe` bilgilerini tutan loader.
- Kabul Kriteri: registry dosyası yüklenebilmeli, `SourceRegistry::get_by_asset_class(class)` çalışmalı ve en az 3 örnek kaynak metadata'sı bulunmalı.

2) Regex/heuristic kuralları (M2)
- Dosyalar: `MarketData/config/regex_rules.yaml`, `MarketData/src/heuristics.rs`
- Açıklama: ISIN, FX parite pattern, TR fon kodu kısa-pattern gibi kurallar; kurallar score döndürmeli.
- Kabul Kriteri: unit testler regex sonuçlarını doğrulamalı (örn. ISIN yüksek puan, TR fon kısmen düşük puan).

3) Candidate generator (M2)
- Dosyalar: `MarketData/src/candidates.rs`
- Açıklama: Para birimi kökleri için quote combos üretecek, kısa semboller için fon/equity aday listesi çıkaracak.
- Kabul Kriteri: `EUR` inputu için `EURUSD,EURTRY,EURCHF,...` üretmeli; `BTC` ile token adayları doğrulanmalı.

4) Source health & capability checks (M5, altyapı başlangıcı M1)
- Dosyalar: `MarketData/src/source_health.rs`
- Açıklama: Kaynak adaptörünün dataset desteği ve ulaşılabilirliğini kontrol eden per-source health probe; sonuçlar cache'lenir.
- Kabul Kriteri: Basit HTTP probe çalışmalı; failed kaynaklar seçimden elenmeli.

5) SourceSelector (M3)
- Dosyalar: `MarketData/src/source_selector.rs`
- Açıklama: `select(symbol, dataset, opts)` API'si; adımlar: override → registry → heuristics → candidate generation → filter by capability & health → scoring → return top-N + explain.
- Kabul Kriteri: Unit testler seçimi doğrulamalı; `--explain-source` çıktısı açıkça hangi kuralın seçtiğini göstermeli.

6) CLI ve Hub/Etl entegrasyonu (M4)
- Dosyalar: `MarketData/src/bin/market_data_bridge.rs`, `MarketData/src/hub.rs`, `MarketData/src/etl.rs`
- Açıklama: CLI'da `--explain-source` ve `--force-source` flagleri; `Etl::fetch` ve `DataHub::ingest` çağrılarında selection zorlanacak.
- Kabul Kriteri: CLI dry-run explain senaryoları test edilmeli; otomatik seçim dataset uyumsuzluğu durumunda hata/uyarı verilmeli.

7) Tests & CI (M5)
- Testler: `tests/source_detection_tests.rs`, güncellenmiş `tests/bridge_cli_tests.rs` için explain/force testleri.
- CI: `.github/workflows/marketdata-rust-ci.yml` içine mapping-test job'ı eklenmeli.
- Kabul Kriteri: Tüm yeni unit/integ testler CI'de geçmeli.

Estimates (rough):
- M1 — 0.5–1 day
- M2 — 1–2 days
- M3 — 1–2 days
- M4 — 1 day
- M5 — 0.5–1 day
- Tests + CI + polish — 1 day

Toplam: 4–8 iş günü (1 geliştirici), test kapsamına ve entegrasyon zorluklarına bağlı olarak.

Testing strategy
- Unit tests: heuristics, candidate generation, scoring determinism.
- Integration tests: CLI explain/force scenarios and `DataHub` selection enforcement.
- End-to-end: small smoke-run that uses pinned tefas/builds when `--features tefas` toggled (opt-in in CI).

CI/Deployment
- Yeni mapping tests job: run default tests + mapping unit tests.
- Optional job matrix: run once with `--features tefas` and mocked/pinned tefas build in isolated scheduled run.

Rollback & Maintenance
- Feature toggle: expose `--disable-source-mapping` to quickly revert to legacy behavior if needed.
- Versioned `MarketData/config/source_mapping.yaml` with changelog for audit.

Communication & PR notes
- Branch: `feat/source-mapping` (create PR after M3+ M4 completed). PR must include migration notes and sample `source_mapping.yaml`.

Immediate next action (I will start after your confirmation):
1. Create `MarketData/src/source_registry.rs` skeleton + example `MarketData/config/source_metadata.yaml` and open a WIP commit on branch `feat/source-mapping`.
2. Wire `SourceRegistry::load_from_config` and basic unit tests for loader.

Küçük not: bu planın her adımında `symbol_overrides` güncellemesiyle canlı veriye müdahale edilebilir; production'da önce staging ile test edin.

---
Güncelleme kaydedildi: `MarketData/docs/source_mapping.md` (detaylı uygulama planı eklendi)
