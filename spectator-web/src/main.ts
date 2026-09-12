import {
  Beer,
  Clock3,
  createIcons,
  Flag,
  Heart,
  Radio,
  RefreshCw,
  Trophy,
  Users,
  WifiOff,
} from "lucide";
import { marked } from "marked";

import rulesMarkdown from "../../docs/Rules_Brackets.md?raw";
import "./style.css";

const SNAPSHOT_URL =
  import.meta.env.VITE_SNAPSHOT_URL ??
  "https://beeriokartbracket-api.beeriokart.workers.dev/snapshot";
const REFRESH_INTERVAL_MS = 15_000;

type Ruleset = "vanilla" | "beerio";
type StandingStatus = "racing" | "advanced" | "eliminated";
type HeatStatus = "waiting" | "ready" | "active" | "complete";
type Placement = { result: "placed"; position: number } | { result: "disqualified" };

interface RacerSlot {
  slot: number;
  racer_name: string;
  placement: Placement | null;
}

interface Race {
  label: string;
  ruleset: Ruleset;
  racers: RacerSlot[];
}

interface RegistrationState {
  participants: string[];
}

interface PoolStanding {
  rank: number;
  racer_name: string;
  points: number;
  races_completed: number;
  status: StandingStatus;
}

interface PoolsState {
  current_round: number;
  total_rounds: number;
  standings: PoolStanding[];
}

interface BracketHeat {
  id: string;
  status: HeatStatus;
  racers: string[];
  races: Race[];
}

interface BracketRound {
  label: string;
  heats: BracketHeat[];
}

interface BracketState {
  winners: BracketRound[];
  losers: BracketRound[];
  winners_finalists: string[];
  losers_finalists: string[];
}

interface GauntletState {
  racers: Array<{
    racer_name: string;
    lives: number;
    placement: Placement | null;
  }>;
  completed_races: Race[];
}

interface TournamentResult {
  racer_name: string;
  placement: Placement;
}

type TournamentPhase =
  | { phase: "registration"; state: RegistrationState }
  | { phase: "pools"; state: PoolsState }
  | { phase: "bracket"; state: BracketState }
  | { phase: "gauntlet"; state: GauntletState }
  | { phase: "complete"; state: TournamentResult[] };

interface Snapshot {
  schema_version: number;
  revision: number;
  published_at_unix_ms: number;
  tournament_name: string;
  active_race: Race | null;
  tournament: TournamentPhase;
}

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("Application root is missing");
const applicationRoot = app;

let snapshot: Snapshot | null = null;
let loadError: string | null = null;
let loading = true;
let bracketSide: "winners" | "losers" = "winners";

function escapeHtml(value: unknown): string {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function icon(name: string, label?: string): string {
  const aria = label ? `aria-label="${escapeHtml(label)}"` : "aria-hidden=\"true\"";
  return `<i data-lucide="${name}" ${aria}></i>`;
}

function renderNavigation(activePage: "live" | "rules"): string {
  return `
    <nav class="site-nav" aria-label="Tournament pages">
      <a href="/" class="${activePage === "live" ? "active" : ""}">Live</a>
      <a href="/rules" class="${activePage === "rules" ? "active" : ""}">Rules</a>
    </nav>`;
}

function renderRulesPage(): void {
  document.title = "Tournament Rules | Beerio Kart Invitational";
  applicationRoot.innerHTML = `
    <header class="site-header">
      <div class="header-inner">
        <img src="/assets/beerio_kart_logo.png" alt="Beerio Kart Invitational" />
        <div class="event-title"><p>Official tournament guide</p><h1>Rules</h1></div>
        <div class="header-actions">${renderNavigation("rules")}</div>
      </div>
    </header>
    <div class="rules-shell">
      <article class="rules-document">${marked.parse(rulesMarkdown, { async: false })}</article>
    </div>
    <footer><img src="/assets/beerio_kart_logo.png" alt="" /><p>Northwest Beerio Kart Invitational</p></footer>`;
}

function phaseLabel(phase: TournamentPhase["phase"]): string {
  return {
    registration: "Registration",
    pools: "Pool Play",
    bracket: "Double Elimination",
    gauntlet: "Grand Finals Gauntlet",
    complete: "Tournament Complete",
  }[phase];
}

function rulesetLabel(ruleset: Ruleset): string {
  return ruleset === "beerio" ? "Beerio Kart" : "Vanilla";
}

function placementLabel(placement: Placement | null): string {
  if (!placement) return "—";
  if (placement.result === "disqualified") return "DQ";
  const position = placement.position;
  const mod100 = position % 100;
  const suffix = mod100 >= 11 && mod100 <= 13
    ? "th"
    : position % 10 === 1
      ? "st"
      : position % 10 === 2
        ? "nd"
        : position % 10 === 3
          ? "rd"
          : "th";
  return `${position}${suffix}`;
}

function renderRace(race: Race): string {
  return `
    <section class="live-race" aria-labelledby="live-heading">
      <div class="live-race__heading">
        <div>
          <p class="eyebrow live-label">${icon("radio")} Live now</p>
          <h2 id="live-heading">${escapeHtml(race.label)}</h2>
        </div>
        <span class="ruleset ruleset--${race.ruleset}">
          ${icon(race.ruleset === "beerio" ? "beer" : "flag")}
          ${rulesetLabel(race.ruleset)}
        </span>
      </div>
      <ol class="racer-grid">
        ${race.racers
          .map(
            (racer) => `
              <li>
                <span class="slot">${String(racer.slot).padStart(2, "0")}</span>
                <strong>${escapeHtml(racer.racer_name)}</strong>
                ${racer.placement ? `<span class="place">${placementLabel(racer.placement)}</span>` : ""}
              </li>`,
          )
          .join("")}
      </ol>
    </section>`;
}

function renderRegistration(state: RegistrationState): string {
  return `
    <section class="section-block">
      <div class="section-heading">
        <div><p class="eyebrow">Roster</p><h2>${state.participants.length} racers checked in</h2></div>
        ${icon("users")}
      </div>
      <div class="name-list">
        ${state.participants.map((name, index) => `<div><span>${index + 1}</span>${escapeHtml(name)}</div>`).join("")}
      </div>
    </section>`;
}

function renderPools(state: PoolsState): string {
  const progress = state.total_rounds === 0 ? 0 : (state.current_round / state.total_rounds) * 100;
  return `
    <section class="section-block">
      <div class="section-heading">
        <div><p class="eyebrow">Standings</p><h2>Round ${state.current_round} of ${state.total_rounds}</h2></div>
        <div class="progress-ring" style="--progress:${progress}%"><strong>${Math.round(progress)}%</strong></div>
      </div>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Rank</th><th>Racer</th><th>Points</th><th>Races</th><th>Status</th></tr></thead>
          <tbody>
            ${state.standings
              .map(
                (row) => `<tr>
                  <td class="rank">${row.rank}</td>
                  <td><strong>${escapeHtml(row.racer_name)}</strong></td>
                  <td class="score">${row.points}</td>
                  <td>${row.races_completed}</td>
                  <td><span class="status status--${row.status}">${row.status}</span></td>
                </tr>`,
              )
              .join("")}
          </tbody>
        </table>
      </div>
    </section>`;
}

function renderBracket(state: BracketState): string {
  const rounds = state[bracketSide];
  const finalists = bracketSide === "winners" ? state.winners_finalists : state.losers_finalists;
  return `
    <section class="section-block bracket-section">
      <div class="section-heading">
        <div><p class="eyebrow">Bracket</p><h2>Road to the gauntlet</h2></div>
        ${icon("trophy")}
      </div>
      <div class="segment" role="group" aria-label="Bracket side">
        <button data-side="winners" class="${bracketSide === "winners" ? "active" : ""}">Winners</button>
        <button data-side="losers" class="${bracketSide === "losers" ? "active" : ""}">Losers</button>
      </div>
      <div class="rounds">
        ${rounds.map(renderRound).join("")}
      </div>
      ${finalists.length ? `<div class="finalists"><p class="eyebrow">Finalists secured</p>${finalists.map((name) => `<strong>${escapeHtml(name)}</strong>`).join("")}</div>` : ""}
    </section>`;
}

function renderRound(round: BracketRound, index: number): string {
  return `
    <details class="round" ${round.heats.some((heat) => heat.status === "active") || index === 0 ? "open" : ""}>
      <summary><span>${escapeHtml(round.label)}</span><small>${round.heats.length} ${round.heats.length === 1 ? "heat" : "heats"}</small></summary>
      <div class="heats">
        ${round.heats
          .map(
            (heat, heatIndex) => `<article class="heat heat--${heat.status}">
              <header><strong>Heat ${heatIndex + 1}</strong><span class="status status--${heat.status}">${heat.status}</span></header>
              ${heat.racers.length
                ? `<ul>${heat.racers.map((name) => `<li>${escapeHtml(name)}</li>`).join("")}</ul>`
                : `<p class="waiting">Awaiting feeder results</p>`}
              ${heat.races.length ? `<p class="race-count">${heat.races.length} ${heat.races.length === 1 ? "race" : "races"} complete</p>` : ""}
            </article>`,
          )
          .join("")}
      </div>
    </details>`;
}

function renderGauntlet(state: GauntletState): string {
  const sorted = [...state.racers].sort((a, b) => {
    if (a.placement && b.placement) return placementNumber(a.placement) - placementNumber(b.placement);
    if (a.placement) return 1;
    if (b.placement) return -1;
    return b.lives - a.lives;
  });
  return `
    <section class="section-block">
      <div class="section-heading">
        <div><p class="eyebrow">Last racer standing</p><h2>Gauntlet lives</h2></div>
        ${icon("heart")}
      </div>
      <div class="gauntlet-grid">
        ${sorted
          .map(
            (racer) => `<article class="gauntlet-racer ${racer.placement ? "eliminated" : ""}">
              <div><strong>${escapeHtml(racer.racer_name)}</strong>${racer.placement ? `<span>${placementLabel(racer.placement)} place</span>` : `<span>Still racing</span>`}</div>
              <div class="lives" aria-label="${racer.lives} lives">${racer.placement ? placementLabel(racer.placement) : `${icon("heart")} ${racer.lives}`}</div>
            </article>`,
          )
          .join("")}
      </div>
      ${state.completed_races.length ? `<p class="history-note">${state.completed_races.length} gauntlet races complete</p>` : ""}
    </section>`;
}

function placementNumber(placement: Placement): number {
  return placement.result === "placed" ? placement.position : Number.MAX_SAFE_INTEGER;
}

function renderComplete(results: TournamentResult[]): string {
  const sorted = [...results].sort((a, b) => placementNumber(a.placement) - placementNumber(b.placement));
  const podiumOrder = [sorted[1], sorted[0], sorted[2]].filter(Boolean);
  return `
    <section class="section-block complete-section">
      <div class="section-heading">
        <div><p class="eyebrow">Final results</p><h2>A champion is crowned</h2></div>
        ${icon("trophy")}
      </div>
      <div class="podium">
        ${podiumOrder
          .map(
            (result) => `<article class="podium-place podium-place--${placementNumber(result.placement)}">
              <span>${placementLabel(result.placement)}</span>
              <strong>${escapeHtml(result.racer_name)}</strong>
              ${placementNumber(result.placement) === 1 ? `<small>Champion</small>` : ""}
            </article>`,
          )
          .join("")}
      </div>
      ${sorted.length > 3 ? `<div class="final-table">${sorted.slice(3).map((result) => `<div><span>${placementLabel(result.placement)}</span><strong>${escapeHtml(result.racer_name)}</strong></div>`).join("")}</div>` : ""}
    </section>`;
}

function renderPhase(tournament: TournamentPhase): string {
  switch (tournament.phase) {
    case "registration": return renderRegistration(tournament.state);
    case "pools": return renderPools(tournament.state);
    case "bracket": return renderBracket(tournament.state);
    case "gauntlet": return renderGauntlet(tournament.state);
    case "complete": return renderComplete(tournament.state);
  }
}

function render(): void {
  if (!snapshot && loading) {
    applicationRoot.innerHTML = `<div class="center-state"><span class="loader"></span><strong>Loading the tournament</strong></div>`;
    return;
  }
  if (!snapshot) {
    applicationRoot.innerHTML = `<div class="center-state error-state">${icon("wifi-off")}<strong>No live tournament yet</strong><p>${escapeHtml(loadError ?? "Check back shortly.")}</p><button id="retry">${icon("refresh-cw")} Retry</button><a class="state-link" href="/rules">View tournament rules</a></div>`;
    activateIcons();
    document.querySelector("#retry")?.addEventListener("click", () => void refresh());
    return;
  }

  const updated = new Date(snapshot.published_at_unix_ms);
  const stale = Date.now() - snapshot.published_at_unix_ms > 60_000;
  applicationRoot.innerHTML = `
    <header class="site-header">
      <div class="header-inner">
        <img src="/assets/beerio_kart_logo.png" alt="Beerio Kart Invitational" />
        <div class="event-title"><p>${escapeHtml(phaseLabel(snapshot.tournament.phase))}</p><h1>${escapeHtml(snapshot.tournament_name)}</h1></div>
        <div class="header-actions">
          ${renderNavigation("live")}
          <button class="icon-button ${loading ? "spinning" : ""}" id="refresh" title="Refresh tournament" aria-label="Refresh tournament">${icon("refresh-cw")}</button>
        </div>
      </div>
    </header>
    <div class="update-bar ${loadError || stale ? "update-bar--warning" : ""}">
      <span class="live-dot"></span>
      <span>${loadError ? "Connection interrupted · showing last update" : stale ? "Update delayed" : "Live tournament feed"}</span>
      <time datetime="${updated.toISOString()}">${icon("clock-3")} ${updated.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</time>
    </div>
    <div class="page-shell">
      ${snapshot.active_race ? renderRace(snapshot.active_race) : ""}
      ${renderPhase(snapshot.tournament)}
    </div>
    <footer><img src="/assets/beerio_kart_logo.png" alt="" /><p>Northwest Beerio Kart Invitational</p></footer>`;

  activateIcons();
  document.querySelector("#refresh")?.addEventListener("click", () => void refresh());
  document.querySelectorAll<HTMLButtonElement>("[data-side]").forEach((button) => {
    button.addEventListener("click", () => {
      bracketSide = button.dataset.side as "winners" | "losers";
      render();
    });
  });
}

function activateIcons(): void {
  createIcons({
    attrs: { "stroke-width": 2 },
    icons: { Beer, Clock3, Flag, Heart, Radio, RefreshCw, Trophy, Users, WifiOff },
  });
}

function isSnapshot(value: unknown): value is Snapshot {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<Snapshot>;
  return candidate.schema_version === 1 && typeof candidate.tournament_name === "string" && !!candidate.tournament;
}

async function refresh(): Promise<void> {
  loading = true;
  render();
  try {
    const response = await fetch(SNAPSHOT_URL, { cache: "no-store" });
    if (!response.ok) throw new Error(response.status === 404 ? "The event has not published yet." : `Feed unavailable (${response.status})`);
    const next: unknown = await response.json();
    if (!isSnapshot(next)) throw new Error("The live feed returned an unsupported format.");
    snapshot = next;
    loadError = null;
  } catch (error) {
    loadError = error instanceof Error ? error.message : "Unable to reach the live feed.";
  } finally {
    loading = false;
    render();
  }
}

if (window.location.pathname.replace(/\/+$/, "") === "/rules") {
  renderRulesPage();
} else {
  setInterval(() => {
    if (document.visibilityState === "visible") void refresh();
  }, REFRESH_INTERVAL_MS);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void refresh();
  });

  render();
  void refresh();
}