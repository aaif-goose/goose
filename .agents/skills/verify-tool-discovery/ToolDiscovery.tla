---------------------------- MODULE ToolDiscovery ----------------------------
(***************************************************************************)
(* Progressive tool discovery in one goose session that defers tools       *)
(* (`session/new` with `_meta.deferTools: true`, code mode off).           *)
(*                                                                         *)
(* Every variable is saved session state, so rebuilding the state machine  *)
(* from the session between any two steps (pipeline.rs, reconstruction)    *)
(* changes nothing here:                                                   *)
(*   enabled  the session's extensions, as the ExtensionLease sees them    *)
(*   loaded   ToolDiscoveryState.loaded_tools (extension_data,             *)
(*            tool_discovery.v0)                                           *)
(*   sent     the tools of the last inference request, as ops_llm.rs       *)
(*            records them in the assistant message's `advertised_tools`   *)
(*            note                                                         *)
(*   phase, loads, call   the tool requests of that assistant message      *)
(*            that no tool response answers yet                            *)
(*                                                                         *)
(* Auto-compaction and /compact write only the conversation, after every   *)
(* tool request is answered (CompactionOperation runs after the tool       *)
(* operations), so they are stutter steps of this model.                   *)
(*                                                                         *)
(* promised, fresh and the five flags after them are history: they record  *)
(* what the model was told and what each request looked like, so the      *)
(* rules can be invariants.                                                *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    Limit,      \* MAX_ADVERTISED_TOOLS (128), scaled down with the catalog
    OpTools     \* tools operations add to every request of a deferring session

(* The catalog, scaled down. Ids sort in the same order as the names, so  *)
(* sorting by id is the code's sort by name.                               *)
Name == <<"developer__shell", "github__get_me", "github__list_prs",
          "linear__get_issue", "linear__save_issue",
          "load_skill", "load_tools", "search_tools">>
Platform   == {1}                        \* goose's own platform tools
Owner      == [t \in 2..5 |-> IF t <= 3 THEN "github" ELSE "linear"]
ExtTools   == DOMAIN Owner               \* MCP extension tools
Ext        == {"github", "linear"}
AlwaysLoad == {2}                        \* _meta["goose/alwaysLoad"]: true

VARIABLES
    enabled, loaded, sent, phase, loads, call,
    promised, fresh,
    everyFits, loadFits, prefixKept, promisesKept, ranUnsent

vars == <<enabled, loaded, sent, phase, loads, call, promised, fresh,
          everyFits, loadFits, prefixKept, promisesKept, ranUnsent>>
history == <<everyFits, loadFits, prefixKept, promisesKept, ranUnsent>>

Range(s) == {s[i] : i \in DOMAIN s}
IndexOf(s, x) == CHOOSE i \in DOMAIN s : s[i] = x
IsPrefix(s, t) == Len(s) <= Len(t) /\ SubSeq(t, 1, Len(s)) = s

(* ExtensionLease::tools_excluding(skills), kept to model-visible tools.   *)
Present(en) == Platform \cup {t \in ExtTools : Owner[t] \in en}

(* ExtensionLease::deferrable_tools: model-visible tools of MCP            *)
(* extensions without goose/alwaysLoad.                                    *)
Deferrable(en) == {t \in ExtTools \ AlwaysLoad : Owner[t] \in en}

(* ToolExecutionOperation::inference_tools drops unloaded_tools; the       *)
(* skills and tool discovery operations add their tools.                   *)
Offered(en, ld) == (Present(en) \ (Deferrable(en) \ Range(ld))) \cup OpTools

(* reply_parts::prepare_inference_tools sorts by (position in loaded,      *)
(* name), and None sorts before Some.                                      *)
Key(t, ld) == IF t \in Range(ld) THEN <<1, IndexOf(ld, t), t>> ELSE <<0, 0, t>>
Before(a, b) ==
    \/ a[1] < b[1]
    \/ a[1] = b[1] /\ a[2] < b[2]
    \/ a[1] = b[1] /\ a[2] = b[2] /\ a[3] < b[3]
SortBy(S, ld) ==
    [i \in 1..Cardinality(S) |->
        CHOOSE t \in S :
            Cardinality({u \in S : Before(Key(u, ld), Key(t, ld))}) = i - 1]
Request(en, ld) == SortBy(Offered(en, ld), ld)

(* One `load_tools` call: an exact name, or `<extension>__*`.              *)
NameLoad(t) == [kind |-> "name", tool |-> t, ext |-> "none"]
ExtLoad(e)  == [kind |-> "all", tool |-> 0, ext |-> e]
LoadCalls == {NameLoad(t) : t \in ExtTools} \cup {ExtLoad(e) : e \in Ext}
LoadBatches == {<<>>} \cup {<<a>> : a \in LoadCalls}
                      \cup {<<a, b>> : a, b \in LoadCalls}

Matches(r, en) ==
    IF r.kind = "all"
    THEN {t \in ExtTools \cap Present(en) : Owner[t] = r.ext}
    ELSE {r.tool} \cap Present(en)

(* tool_discovery::resolve_load. `advertised` is how many tools the next   *)
(* request carries without this load. A load past Limit loads nothing and  *)
(* reports every match as rejected.                                        *)
Resolve(r, en, ld, advertised) ==
    LET m     == Matches(r, en)
        newly == {t \in m : t \in Deferrable(en) /\ t \notin Range(ld)}
        fits  == advertised + Cardinality(newly) <= Limit
    IN  [ld   |-> IF fits THEN ld \o SortBy(newly, <<>>) ELSE ld,
         told |-> IF fits THEN m ELSE {}]

(* ToolDiscoveryOperation::run answers every pending load_tools call of    *)
(* the batch in one step. Each call counts the tools the requesting        *)
(* inference sent plus what earlier calls of the same batch loaded.        *)
RECURSIVE RunLoads(_, _, _, _, _)
RunLoads(ls, en, ld, orig, told) ==
    IF ls = <<>> THEN [ld |-> ld, told |-> told]
    ELSE LET loadedThisBatch == Len(ld) - Len(orig)
             r == Resolve(Head(ls), en, ld, Len(sent) + loadedThisBatch)
         IN  RunLoads(Tail(ls), en, r.ld, orig, told \cup r.told)

Init ==
    /\ enabled = Ext
    /\ loaded = <<>>
    /\ sent = <<>>
    /\ phase = "idle"
    /\ loads = <<>>
    /\ call = 0
    /\ promised = {}
    /\ fresh = TRUE
    /\ everyFits = TRUE /\ loadFits = TRUE /\ prefixKept = TRUE
    /\ promisesKept = TRUE /\ ranUnsent = FALSE

(* An inference: goose sends Request(enabled, loaded), and the model       *)
(* answers with up to two load_tools calls and up to one extension tool    *)
(* call, which may name a tool this request didn't send.                   *)
Infer ==
    /\ phase = "idle"
    /\ LET S == Request(enabled, loaded) IN
        /\ everyFits'    = (Len(S) <= Limit)
        /\ loadFits'     = (fresh \/ Len(sent) > Limit \/ Len(S) <= Limit)
        /\ prefixKept'   = (fresh \/ IsPrefix(sent, S))
        /\ promisesKept' = (fresh \/ promised \subseteq Range(S))
        /\ sent' = S
    /\ \E ls \in LoadBatches, c \in {0} \cup ExtTools :
        /\ loads' = ls
        /\ call' = c
        /\ phase' = IF ls # <<>> THEN "loads" ELSE IF c # 0 THEN "calls" ELSE "idle"
    /\ promised' = {}
    /\ fresh' = FALSE
    /\ UNCHANGED <<enabled, loaded, ranUnsent>>

DiscoveryRun ==
    /\ phase = "loads"
    /\ LET r == RunLoads(loads, enabled, loaded, loaded, {}) IN
        /\ loaded' = r.ld
        /\ promised' = promised \cup r.told
    /\ loads' = <<>>
    /\ phase' = IF call # 0 THEN "calls" ELSE "idle"
    /\ UNCHANGED <<enabled, sent, call, fresh, history>>

(* ToolExecutionOperation runs a call only when the requesting inference   *)
(* advertised the tool and the lease knows it. Otherwise                   *)
(* RemainingToolCallsOperation answers, and says "loaded in the same       *)
(* response, call it again in your next response" when the tool is a       *)
(* loaded deferred tool.                                                   *)
CallAnswered ==
    /\ phase = "calls"
    /\ LET runs == call \in Range(sent) /\ call \in Present(enabled)
           retryNext == ~runs /\ call \in Deferrable(enabled)
                              /\ call \in Range(loaded)
       IN  /\ ranUnsent' = (ranUnsent \/ (runs /\ call \notin Range(sent)))
           /\ promised' = IF retryNext THEN promised \cup {call} ELSE promised
    /\ call' = 0
    /\ phase' = "idle"
    /\ UNCHANGED <<enabled, loaded, sent, loads, fresh,
                   everyFits, loadFits, prefixKept, promisesKept>>

(* /clear: CompactionOperation::clear empties the conversation and         *)
(* loaded_tools, and the session keeps deferring.                          *)
Clear ==
    /\ loaded' = <<>>
    /\ loads' = <<>>
    /\ call' = 0
    /\ phase' = "idle"
    /\ promised' = {}
    /\ fresh' = TRUE
    /\ UNCHANGED <<enabled, sent, history>>

Flip(en, e) == IF e \in en THEN en \ {e} ELSE en \cup {e}

(* The session's extensions change between requests: the model's           *)
(* manage_extensions call took effect, or the ACP client sent another       *)
(* selection with a new turn. Tool calls never run against a lease other   *)
(* than their inference's: only inference sets the shared lease            *)
(* (inference_preparation.rs), and a new turn or a restart clears it.      *)
ToggleExtension(e) ==
    /\ phase = "idle"
    /\ enabled' = Flip(enabled, e)
    /\ fresh' = TRUE
    /\ UNCHANGED <<loaded, sent, phase, loads, call, promised, history>>

(* goose stops, or a new turn starts, before the pending calls are         *)
(* answered. With no lease, ToolDiscoveryOperation and                     *)
(* ToolExecutionOperation answer them with EXPIRED_APPROVAL_RESPONSE, so   *)
(* nothing loads or runs.                                                  *)
LeaseLost ==
    /\ phase # "idle"
    /\ loads' = <<>>
    /\ call' = 0
    /\ phase' = "idle"
    /\ UNCHANGED <<enabled, loaded, sent, promised, fresh, history>>

Next ==
    \/ Infer
    \/ DiscoveryRun
    \/ CallAnswered
    \/ Clear
    \/ LeaseLost
    \/ \E e \in Ext : ToggleExtension(e)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Rules. Each one is checked on every reachable state.                    *)

\* No request carries more than Limit tools.
EveryRequestFits == everyFits

\* A request that follows a request within Limit, with no extension change
\* or /clear in between, is within Limit too: loads alone never pass it.
NoLoadPushesPastLimit == loadFits

\* With no extension change or /clear in between, the tools of the earlier
\* request are a prefix of the next one's, so a load only appends.
LoadsOnlyAppend == prefixKept

\* A tool the model was told is loaded, or told to call again next time,
\* is in the next request, unless extensions changed or /clear ran.
LoadedToolsAreSent == promisesKept

\* A tool runs only if the request that called it sent it. A tool loaded in
\* the same response doesn't run.
UnsentToolsNeverRun == ~ranUnsent

\* loaded_tools names each tool once.
LoadsAreDistinct == Cardinality(Range(loaded)) = Len(loaded)

\* Cuts off states only a broken resolve_load reaches: a correct one never
\* records more names than there are extension tools.
Bounded == Len(loaded) <= Cardinality(ExtTools)
=============================================================================
