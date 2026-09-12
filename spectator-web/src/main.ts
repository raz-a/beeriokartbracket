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
  expected_size?: number;
  racers: string[];
  races: Race[];
  feeders?: Array<{
    source_heat_id: string;
    source: "winners" | "losers";
    racer_count: number;
    resolved: boolean;
  }>;
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
let loading = false;
let hasRequestedSnapshot = false;
let bracketResizeObserver: ResizeObserver | null = null;

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
            (racer, index) => `
              <li>
                <span class="slot">${String(racer.slot).padStart(2, "0")}</span>
                <strong class="racer-name player-color--${index % 8}">${escapeHtml(racer.racer_name)}</strong>
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
  return `
    <section class="section-block bracket-section">
      <div class="section-heading">
        <div><p class="eyebrow">Bracket</p><h2>Road to the gauntlet</h2></div>
        ${icon("trophy")}
      </div>
      <div class="bracket-lanes">
        ${renderBracketLane("Winners bracket", state.winners, state.winners_finalists, "winners")}
        ${renderBracketLane("Losers bracket", state.losers, state.losers_finalists, "losers")}
      </div>
    </section>`;
}

function renderBracketLane(
  heading: string,
  rounds: BracketRound[],
  finalists: string[],
  side: "winners" | "losers",
): string {
  return `
    <section class="bracket-lane bracket-lane--${side}" aria-labelledby="${side}-bracket-heading">
      <h3 id="${side}-bracket-heading">${heading}</h3>
      <nav class="bracket-round-nav" aria-label="${heading} rounds">
        ${rounds.map((round, index) => `<button type="button" data-round-index="${index}" ${index === 0 ? 'aria-current="true"' : ""}>${escapeHtml(bracketRoundLabel(round, index, side, rounds.length))}</button>`).join("")}
      </nav>
      <div class="bracket-tree-scroll">
        <div class="bracket-tree" data-bracket-side="${side}">
          <svg class="bracket-connectors" aria-hidden="true"></svg>
          ${rounds.map((round, index) => renderBracketRound(round, index, side, rounds.length)).join("")}
        </div>
      </div>
      ${finalists.length ? `<div class="finalists"><p class="eyebrow">Finalists secured</p>${finalists.map((name) => `<strong>${escapeHtml(name)}</strong>`).join("")}</div>` : ""}
    </section>`;
}

function renderBracketRound(
  round: BracketRound,
  index: number,
  side: "winners" | "losers",
  roundCount: number,
): string {
  const label = bracketRoundLabel(round, index, side, roundCount);
  return `
    <section class="bracket-round-column" data-round-index="${index}">
      <header class="bracket-round-heading">
        <strong>${escapeHtml(label)}</strong>
        <span>${round.heats.length} ${round.heats.length === 1 ? "heat" : "heats"}</span>
      </header>
      <div class="bracket-round-heats">
        ${round.heats.map(renderBracketHeat).join("")}
      </div>
    </section>`;
}

function bracketRoundLabel(
  round: BracketRound,
  index: number,
  side: "winners" | "losers",
  roundCount: number,
): string {
  return side === "winners" && index === roundCount - 1
    ? "Gauntlet Qualifiers"
    : round.label;
}

function renderBracketHeat(heat: BracketHeat, heatIndex: number): string {
  const expectedSize = Math.max(heat.expected_size ?? heat.racers.length, heat.racers.length);
  const placeholders = (heat.feeders ?? [])
    .filter((feeder) => !feeder.resolved)
    .flatMap((feeder) => Array.from({ length: feeder.racer_count }, () => ({
      label: `${feeder.source === "winners" ? "Winner" : "Loser"} from ${heatLabel(feeder.source_heat_id)}`,
      source: feeder.source,
    })));
  while (heat.racers.length + placeholders.length < expectedSize) {
    placeholders.push({ label: "Awaiting racer", source: "winners" });
  }

  return `
    <article class="bracket-heat heat--${heat.status}" data-heat-id="${escapeHtml(heat.id)}" data-feeders="${escapeHtml((heat.feeders ?? []).map((feeder) => feeder.source_heat_id).join(","))}">
      <header>
        <strong>Heat ${heatIndex + 1}</strong>
        <span class="status status--${heat.status}">${heat.status}</span>
      </header>
      <ol class="bracket-roster">
        ${heat.racers.map((name, index) => `<li><span class="tree-marker">•</span><strong class="player-color--${index % 8}">${escapeHtml(name)}</strong></li>`).join("")}
        ${placeholders.map((placeholder) => `<li class="feeder-slot feeder-slot--${placeholder.source}"><span class="tree-marker">${placeholder.source === "winners" ? "W" : "L"}</span><span>${escapeHtml(placeholder.label)}</span></li>`).join("")}
      </ol>
      ${renderHeatResults(heat)}
    </article>`;
}

function renderHeatResults(heat: BracketHeat): string {
  if (heat.races.length === 0) return "";
  const racers = heat.racers.length > 0
    ? heat.racers
    : heat.races[0]?.racers.map((racer) => racer.racer_name) ?? [];
  return `
    <details class="heat-results" open>
      <summary>${heat.races.length} ${heat.races.length === 1 ? "race" : "races"} recorded</summary>
      <div class="heat-results-scroll">
        <table style="width: ${88 + heat.races.length * 38}px">
          <thead><tr><th>Racer</th>${heat.races.map((race, index) => `<th title="${escapeHtml(race.label)} · ${rulesetLabel(race.ruleset)}">${race.ruleset === "beerio" ? "B" : "R"}${index + 1}</th>`).join("")}</tr></thead>
          <tbody>${racers.map((name) => `<tr><td>${escapeHtml(name)}</td>${heat.races.map((race) => {
            const result = race.racers.find((racer) => racer.racer_name === name);
            return `<td>${placementLabel(result?.placement ?? null)}</td>`;
          }).join("")}</tr>`).join("")}</tbody>
        </table>
      </div>
    </details>`;
}

function heatLabel(id: string): string {
  const match = /^(winners|losers)-(\d+)-(\d+)$/.exec(id);
  return match ? `${match[1] === "winners" ? "W" : "L"}${match[2]} H${match[3]}` : id;
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
    applicationRoot.innerHTML = `
      <header class="site-header">
        <div class="header-inner">
          <img src="/assets/beerio_kart_logo.png" alt="Beerio Kart Invitational" />
          <div class="event-title"><p>Live tournament coverage</p><h1>Spectator View</h1></div>
          <div class="header-actions">${renderNavigation("live")}</div>
        </div>
      </header>
      <div class="center-state error-state">
        ${icon(loadError ? "wifi-off" : "radio")}
        <strong>${loadError ? "No live tournament available" : "Ready when you are"}</strong>
        <p>${escapeHtml(loadError ?? "Load the latest published tournament results.")}</p>
        <button id="load-snapshot">${icon("refresh-cw")} ${hasRequestedSnapshot ? "Try again" : "Load live results"}</button>
        <a class="state-link" href="/rules">View tournament rules</a>
      </div>`;
    activateIcons();
    document.querySelector("#load-snapshot")?.addEventListener("click", () => void refresh());
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
          <button class="refresh-button ${loading ? "spinning" : ""}" id="refresh">${icon("refresh-cw")} Refresh</button>
        </div>
      </div>
    </header>
    <div class="update-bar ${loadError || stale ? "update-bar--warning" : ""}">
      <span class="live-dot"></span>
      <span>${loadError ? "Refresh failed · showing last update" : stale ? "Snapshot may be outdated · refresh for latest" : "Latest snapshot"}</span>
      <time datetime="${updated.toISOString()}">${icon("clock-3")} ${updated.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</time>
    </div>
    <div class="page-shell">
      ${snapshot.active_race ? renderRace(snapshot.active_race) : ""}
      ${renderPhase(snapshot.tournament)}
    </div>
    <footer><img src="/assets/beerio_kart_logo.png" alt="" /><p>Northwest Beerio Kart Invitational</p></footer>`;

  activateIcons();
  document.querySelector("#refresh")?.addEventListener("click", () => void refresh());
  initializeBracketTrees();
}

function initializeBracketTrees(): void {
  bracketResizeObserver?.disconnect();
  const trees = document.querySelectorAll<HTMLElement>(".bracket-tree");
  if (trees.length === 0) return;

  const redraw = () => requestAnimationFrame(drawBracketConnections);
  bracketResizeObserver = new ResizeObserver(redraw);
  trees.forEach((tree) => bracketResizeObserver?.observe(tree));
  document.querySelectorAll<HTMLElement>(".bracket-lane").forEach((lane) => {
    const scroller = lane.querySelector<HTMLElement>(".bracket-tree-scroll");
    const navigation = lane.querySelector<HTMLElement>(".bracket-round-nav");
    const rounds = [...lane.querySelectorAll<HTMLElement>(".bracket-round-column")];
    const buttons = [...lane.querySelectorAll<HTMLButtonElement>(".bracket-round-nav button")];
    if (!scroller || rounds.length === 0) return;

    const updateCurrentRound = () => {
      const currentIndex = rounds.reduce((closestIndex, round, index) =>
        Math.abs(round.offsetLeft - scroller.scrollLeft)
          < Math.abs(rounds[closestIndex].offsetLeft - scroller.scrollLeft)
          ? index
          : closestIndex, 0);
      buttons.forEach((button, index) => {
        if (index === currentIndex) button.setAttribute("aria-current", "true");
        else button.removeAttribute("aria-current");
      });
      const currentButton = buttons[currentIndex];
      if (navigation && currentButton) {
        const navigationRect = navigation.getBoundingClientRect();
        const buttonRect = currentButton.getBoundingClientRect();
        const buttonLeft = navigation.scrollLeft + buttonRect.left - navigationRect.left;
        navigation.scrollTo({ left: Math.max(0, buttonLeft - 8), behavior: "smooth" });
      }
    };
    buttons.forEach((button, index) => {
      button.addEventListener("click", () => {
        scroller.scrollTo({ left: rounds[index].offsetLeft, behavior: "smooth" });
      });
    });
    scroller.addEventListener("scroll", updateCurrentRound, { passive: true });
  });
  document.querySelectorAll<HTMLDetailsElement>(".heat-results").forEach((details) => {
    details.addEventListener("toggle", redraw);
  });
  redraw();
}

function drawBracketConnections(): void {
  document.querySelectorAll<HTMLElement>(".bracket-tree").forEach((tree) => {
    const svg = tree.querySelector<SVGSVGElement>(".bracket-connectors");
    if (!svg) return;
    const treeRect = tree.getBoundingClientRect();
    svg.setAttribute("viewBox", `0 0 ${tree.scrollWidth} ${tree.scrollHeight}`);
    svg.replaceChildren();

    const heats = new Map(
      [...tree.querySelectorAll<HTMLElement>(".bracket-heat")]
        .map((heat) => [heat.dataset.heatId, heat] as const),
    );
    heats.forEach((target) => {
      const feederIds = target.dataset.feeders?.split(",").filter(Boolean) ?? [];
      feederIds.forEach((feederId) => {
        const source = heats.get(feederId);
        if (!source) return;
        const sourceRect = source.getBoundingClientRect();
        const targetRect = target.getBoundingClientRect();
        const startX = sourceRect.right - treeRect.left;
        const startY = sourceRect.top + sourceRect.height / 2 - treeRect.top;
        const endX = targetRect.left - treeRect.left;
        const endY = targetRect.top + targetRect.height / 2 - treeRect.top;
        const middleX = startX + (endX - startX) / 2;
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        path.setAttribute("d", `M ${startX} ${startY} H ${middleX} V ${endY} H ${endX}`);
        svg.append(path);
      });
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
  if (loading) return;
  hasRequestedSnapshot = true;
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
  render();
}