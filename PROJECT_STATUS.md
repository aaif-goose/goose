# Status projektu: mid-turn auto-compaction

Data aktualizacji: 2026-09-09

## Cel

Naprawić automatyczne compactowanie kontekstu podczas długiej autonomicznej
pętli `model -> tool -> model`, bez dodatkowej wiadomości użytkownika i bez
rozbijania pary `tool request -> tool result`.

Docelowy PR: https://github.com/aaif-goose/goose/pull/11903

Powiązane zgłoszenie: https://github.com/aaif-goose/goose/issues/11072

## Stan bieżący

Branch roboczy: `work/mid-turn-auto-compact`.

Ostatni commit bazowy: `fc6b44f Project tool-result suffix before compaction`.

Ostatnio potwierdzony stan PR:

- PR był oznaczony jako **ready for review** na wyraźne polecenie użytkownika;
- wszystkie uruchomione joby CI zakończyły się powodzeniem;
- niektóre joby były celowo `skipped` przez konfigurację workflow;
- po tym pojawił się nowy otwarty komentarz P1, opisany w sekcji
  „Pozostałe zadania”.

W chwili sporządzania tego dokumentu nie udało się ponownie odpytać GitHub
z powodu błędu połączenia sieciowego. Powyższy status pochodzi z ostatniego
udanej weryfikacji.

## Zaimplementowane zmiany

### Obie ścieżki agenta

`AGENTS.md` wymaga zgodności legacy loop i state machine. Zmiany objęły obie:

- legacy: `crates/goose/src/agents/agent.rs`;
- state machine: `crates/goose/src/agents/state_machine/ops_compaction.rs`;
- wspólne liczenie kontekstu: `crates/goose/src/context_mgmt/mod.rs`.

### Compactowanie po tool result

- State machine dopuszcza proactive compaction po kompletnej odpowiedzi
  narzędzia, lecz nie pomiędzy requestem narzędzia a jego odpowiedzią.
- Legacy loop wykonuje kontrolę po zapisaniu tool result.
- Test regresyjny sprawdza, że historia zachowuje zgodne identyfikatory
  tool request i tool response.

### Pełniejsze liczenie kontekstu

- Do usage ostatniego inference doliczane są wiadomości dopisane po nim,
  w szczególności tool result i steer.
- Gdy request został przygotowany, liczona jest bieżąca lista narzędzi,
  prompt systemowy i wiadomości, zamiast opierania się wyłącznie na usage
  poprzedniego inference.
- Uwzględniony jest tool-shim, który przenosi schematy narzędzi do promptu.
- Sufiks tool result jest projektowany do `agent_visible_messages()`, aby
  zawartość wyłącznie dla roli User/App nie powodowała zbędnego compactowania.

### Steer i elicitation

- Legacy loop opróżnia queued steer przed kontrolą po tool result.
- Przed replacementem historii legacy reloaduje zapisaną konwersację, aby
  wymiana elicitation request/response nie została usunięta przez compaction.

## Commity

| Commit | Znaczenie |
| --- | --- |
| `ff8291b` | Pierwsza poprawka mid-turn compaction i test regresyjny. |
| `bbb71b2` | Doliczanie wiadomości po inference. |
| `21c069d` | Liczenie przygotowanego requestu przed inference. |
| `8e527f1` | Uwzględnienie queued steer w legacy check. |
| `f9716a5` | Zachowanie historii elicitation przy compaction. |
| `fc6b44f` | Projekcja tool-result suffix do agent-visible content. |

## Wykonana walidacja

Pomyślnie uruchomiono lokalnie w toku prac:

- `cargo test -p goose auto_compacts_after_a_tool_result_before_the_next_inference -- --nocapture`;
- `cargo test -p goose compaction_lifecycle`;
- `cargo fmt --check`;
- `cargo clippy -p goose --all-targets -- -D warnings`.

Wykonano też test integracyjny Ollama z `qwen3.8:27b-32k`, logicznym limitem
24 576 i progiem 50%. Compactowanie nastąpiło w trakcie pętli tool calli, po
przekroczeniu progu, a agent kontynuował pracę bez wiadomości `continue`.

Pełny CI PR był zielony po commitach wcześniejszych niż ostatnio zgłoszony P1.
Po każdej następnej poprawce należy ponownie uruchomić wymagane testy i
poczekać na CI.

Po poprawce P1 lokalnie przeszły `cargo build`, `cargo fmt --check`,
`cargo clippy -p goose --all-targets -- -D warnings` oraz test state-machine
`auto_compacts_after_a_tool_result_before_the_next_inference`. Pełne
`cargo test -p goose` uruchomione 2026-09-09 miało 8 niezwiązanych błędów
środowiskowych/bazowych: konfigurację summon i Ollama, snapshot platform
extensions, jeden test ACP oraz pięć testów JWT, dla których `jsonwebtoken`
nie może wybrać CryptoProvider. Zmiana P1 nie dotyka tych obszarów.

CI dla `97b70ea` zakończyło się zielono. Kolejna poprawka review przywraca
emit drained steer przed compaction replacement i odracza proactive
state-machine compaction po tool result do exact prepared-request hooka.
Następna poprawka review zwalnia blokadę steer po preflight, przed await
provider streamu; steer dodany potem pozostaje w kolejce dla następnego
preflight i nie zmienia już przygotowanego requestu.
Po wykryciu regresji w CI przywrócono state-machine proactive compaction dla
dużych tool resultów; legacy entry path odracza nieprecyzyjny wrapper check
po takim turnie i wykonuje exact prepared-request check przed pierwszym
provider requestem.
Najnowsza poprawka review zachowuje legacy turn budget po compaction restart
i zapisuje queued steer przed terminalnym final output.

## Stan bieżący — 2026-09-09

P1 dotyczący queued steer jest naprawiony i wypchnięty. Preflight legacy
trzyma blokadę kolejki wyłącznie do czasu przygotowania requestu, a następnie
ją zwalnia przed I/O providera; steer dodany później pozostaje dla następnego
preflight. State machine wykonuje odpowiedni prepared-request hook w osobnym
cyklu, więc nie ma analogicznego okna.

Test `legacy_compacts_a_tool_result_before_the_next_inference` został
poprawiony po review: wymusza `GOOSE_STATE_MACHINE=0` pod `env_lock` i
wykonuje tool request oraz wynik w tym samym legacy reply. Provider wywoływany
jest kolejno dla tool requestu, compaction i kontynuacji.

Walidacja ostatniej zmiany:

- `cargo test -p goose legacy_compacts_a_tool_result_before_the_next_inference -- --nocapture` — pass;
- `cargo fmt --check` — pass;
- `cargo clippy -p goose --all-targets -- -D warnings` — pass;
- `git diff --check` — pass.

Pełne CI dla `afb238a` jest zielone. Pełne lokalne `cargo test -p goose`
nadal ma opisane wyżej 8 niezwiązanych, środowiskowych/bazowych błędów.

Poprawka testu jest w `702f9d6`; odpowiedź review została dodana, a wszystkie
wątki są rozwiązane. CI dla tego commitu rozpoczęło się po pushu.

CI dla `b2d6339` zakończyło się zielono. Kolejny P1 state-machine wykazał,
że assistant summary utworzony przez compaction był liczony jako inference
turn. `CompactionOperation` oznacza teraz własne assistant messages jako
synthetic, a `MaxTurnsOperation` pomija je w budżecie. Regresja używa
`max_turns = 2` i wymaga tool requestu, compaction oraz drugiego inference.

Po `1d9b0b7` CI wykazało, że oczekiwanie regresji na ostatnią wiadomość było
błędne: druga inference nastąpiła, a dopiero później pętla słusznie zwróciła
komunikat o wykorzystaniu obu tur. Test sprawdza teraz kontynuację jako
przedostatnią wiadomość i trzy wywołania providera. Wspólny marker synthetic
jest stosowany zarówno przez `CompactionOperation`, jak i
`PreparedRequestCompactionHook`.

CI dla `cced800` zakończyło się zielono. Późniejszy P1 dotyczył image-only
lub document-only promptu przed dużym tool result: compaction nie mogła
zachować tekstowego promptu, więc historia kończyła się assistant summary.
W takim przypadku wspólne `compact_messages` dodaje teraz agent-only user
continuation; dzięki temu oba loopy mają provider-driving suffix bez
naruszania pary tool request/result. Pokrywają to test wspólnego modułu,
state machine i legacy.

P2 po `fde9558` zauważył, że prepared-request preflight state machine nie
uwzględniał tekstu `Available tools` dodawanego dla unknown-tool error tuż
przed inference. Preflight używa teraz tej samej funkcji
`enrich_unclaimed_tool_errors` co provider, przed liczeniem tokenów.

Kolejny P2 rozszerza tę samą zasadę o Toolshim: wspólny builder requestu
wykonuje teraz project, fix, merge i konwersję Toolshim; używają go inference
oraz preflighty legacy i state machine. CI dla `6fedb47` miało jedną
niezwiązaną porażkę native-TLS (`extension_manager_tools_available`), przy
zielonych Rust i rustls-TLS; następny push ponowi całą macierz.

PR #11681 rozwiązuje ten sam problem #11072, lecz jest odrębną i znacznie
szerszą implementacją (recount tokenów oraz refaktoryzacja compaction). Jej
legacy preflight nadal wykonuje asynchroniczny check po drainie steerów, więc
nie obejmuje później znalezionego wyścigu P1.

## Checklista przed merge

- [x] Naprawić P1 dotyczący późnego queued steer.
- [x] Dodać deterministyczny test legacy z in-turn tool call.
- [x] Potwierdzić parzystość z state machine.
- [x] Uruchomić formatowanie, clippy i ukierunkowany test.
- [x] Poczekać na zielone CI dla ostatnio wypchniętego commitu.
- [x] Wypchnąć poprawkę testu, odpowiedzieć na P2 i zamknąć wątek.
- [x] CI dla `b2d6339` jest zielone.
- [x] CI dla `cced800` jest zielone.
- [ ] Wypchnąć Toolshim preflight fix i poczekać na jej CI.
