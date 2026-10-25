// BALD phase 0 — statyczny shell. To nie jest jeszcze Tauri IPC.
// Dane są offline fixtures; guard jest uproszczoną kopią reguł z bse-guard.

const RESULTS = [
  {
    tier: "T5",
    tierNote: "blog / anonim",
    stance: "endorses",
    stanceNote: "wspiera",
    title: "Cichy Balon: czy aktualizacje naprawdę są ukrywane?",
    domain: "example.org › demo › cichy-balon",
    snippet:
      "Blog opisuje rzekomy „cichy balon” — mechanizm ukrywania zmian w oprogramowaniu. Brak dokumentów źródłowych; powołuje się na anonimowe wpisy.",
    published: "2024-11-03",
    score: "0.0164",
    providers: "demo #1",
    availability: null,
  },
  {
    tier: "T3",
    tierNote: "fact-check",
    stance: "debunks",
    stanceNote: "obala",
    title: "Fact-check: „Cichy Balon” nie istnieje",
    domain: "example.org › demo › fact-check-cichy-balon",
    snippet:
      "Redakcja sprawdziła 14 wersji narracji i nie znalazła dowodów na celowe ukrywanie aktualizacji. Wskazuje na mylenie pojęć: supply chain vs. telemetria.",
    published: "2025-02-14",
    score: "0.0161",
    providers: "demo #2",
    availability: null,
  },
  {
    tier: "ARC",
    tierNote: "snapshot",
    stance: "mentions",
    stanceNote: "wzmianka",
    title: "Archiwum: usunięty wątek o „Cichym Balonie”",
    domain: "example.org › demo › archive-thread",
    snippet:
      "Snapshot wątku z forum z 2023 r. Wątek został usunięty przez moderatora; dostępny w Wayback Machine. Zawiera 42 komentarze i odwołania do nieistniejącego raportu.",
    published: "2023-08-21",
    score: "0.0159",
    providers: "demo #3",
    availability: "REMOVED",
  },
  {
    tier: "T1",
    tierNote: "badanie",
    stance: "reports",
    stanceNote: "relacjonuje",
    title: "Transparency of software supply chains — przegląd badań",
    domain: "example.org › demo › supply-chain-review",
    snippet:
      "Przegląd 31 badań o przejrzystości łańcucha dostaw oprogramowania. Omawia telemetrię, reproducible builds i ograniczenia audytu.",
    published: "2025-06-01",
    score: "0.0156",
    providers: "demo #4",
    availability: null,
  },
  {
    tier: "T2",
    tierNote: "analiza",
    stance: "reports",
    stanceNote: "relacjonuje",
    title: "Deplatforming i obieg narracji w Europie Środkowej",
    domain: "example.org › demo › deplatforming",
    snippet:
      "Analiza przypadków usuwania treści i migracji społeczności. Autorzy rozróżniają cenzurę prawną, moderację platform i samo-cenzurę.",
    published: "2024-12-09",
    score: "0.0154",
    providers: "demo #5",
    availability: null,
  },
  {
    tier: "ARC",
    tierNote: "snapshot",
    stance: "mentions",
    stanceNote: "wzmianka",
    title: "Snapshot: strona projektu „Cichy Balon” (2024)",
    domain: "example.org › demo › snapshot-2024",
    snippet:
      "Zarchiwizowana kopia strony projektu. Strona zniknęła w styczniu 2025; snapshot pokazuje deklarację „brak zaufania do dostawców”.",
    published: "2024-09-18",
    score: "0.0152",
    providers: "demo #6",
    availability: "ARCHIVED",
  },
];

const GUARD_RULES = [
  {
    re: /gdzie\s+(on\s+|ona\s+|jego\s+|jej\s+)?(mieszka|pracuje|przebywa)/i,
    id: "pii.where_lives",
  },
  {
    re: /\b(podaj|znajd[zź]|ustal|sprawd[zź])\b.*\b(adres|telefon|pesel|dane osobowe)\b/i,
    id: "pii.request_contact",
  },
  {
    re: /\b(czyj|czyja)\s+(to\s+)?(pesel|adres|telefon|dom)/i,
    id: "pii.whose_is",
  },
  {
    re: /\b(jak\s+(zabi[cć]|skrzywdzi[cć]|pobi[cć])|doxx?(ing)?|swatting?)\b/i,
    id: "harm.how_to",
  },
  {
    re: /\bwhere does (he|she|this person) (live|work)\b/i,
    id: "pii.where_lives_en",
  },
  {
    re: /\b(adres domowy|prywatny adres|miejsce zamieszkania)\b/i,
    id: "pii.private_address",
  },
];

const $ = (id) => document.getElementById(id);
const escapeHtml = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

function checkGuard(query) {
  for (const rule of GUARD_RULES) {
    if (rule.re.test(query)) {
      return rule;
    }
  }
  return null;
}

function badge(text, className) {
  return `<span class="badge ${className}">${escapeHtml(text)}</span>`;
}

function renderResults(query) {
  const list = $("results");
  list.innerHTML = RESULTS.map((item, index) => {
    const availability = item.availability
      ? badge(item.availability, "avail")
      : "";
    const stanceClass =
      item.stance === "debunks" ? "stance debunks" : "stance";
    return `
      <article class="result">
        <div class="result-top">
          ${badge(`${item.tier} · ${item.tierNote}`, "tier")}
          ${badge(item.stanceNote, stanceClass)}
          ${availability}
        </div>
        <h3><a href="#" data-result="${index}">${escapeHtml(item.title)}</a></h3>
        <div class="source">${escapeHtml(item.domain)} · ${escapeHtml(item.published)}</div>
        <p class="snippet">${escapeHtml(item.snippet)}</p>
        <div class="result-foot">
          <span class="score">rrf ${escapeHtml(item.score)}</span>
          <span>${escapeHtml(item.providers)}</span>
          <span>${index + 1} / ${RESULTS.length}</span>
        </div>
      </article>
    `;
  }).join("");
}

function showRefusal(query, rule) {
  $("refusal").hidden = false;
  $("results").innerHTML = "";
  $("results-title").textContent = `Dla: „${query}” · 0 wyników`;
  $("refusal-match").textContent = `rules · ${rule.id} · REFUSE`;
  $("refusal-reason").textContent =
    "To zapytanie próbuje ustalić dane o osobie. BALD dotyczy tematów i twierdzeń — nie osób, adresów, PESEL-u ani wizerunku.";

  const gate = $("gate");
  gate.classList.add("deny");
  gate.innerHTML =
    '<span class="dot"></span> gate: REFUSED <small>rules · 1.0</small>';
}

function showAllowed(query) {
  $("refusal").hidden = true;
  $("results-title").textContent = `Dla: „${query}” · ${RESULTS.length} wyników`;
  renderResults(query);

  const gate = $("gate");
  gate.classList.remove("deny");
  gate.innerHTML =
    '<span class="dot"></span> gate: ALLOW <small>rules · 0.4 ms</small>';
}

function submitQuery(raw) {
  const query = raw.trim();
  if (!query) return;
  $("q").value = query;
  const rule = checkGuard(query);
  if (rule) {
    showRefusal(query, rule);
  } else {
    showAllowed(query);
  }
}

$("search-form").addEventListener("submit", (event) => {
  event.preventDefault();
  submitQuery($("q").value);
});

document.querySelectorAll(".chips button").forEach((button) => {
  button.addEventListener("click", () => submitQuery(button.dataset.q || ""));
});

// W Tauri CSP blokuje inline handlery — linki wyników obsługujemy delegacją.
$("results").addEventListener("click", (event) => {
  const link = event.target.closest("a[data-result]");
  if (link) event.preventDefault();
});

// Stan początkowy.
showAllowed($("q").value);
