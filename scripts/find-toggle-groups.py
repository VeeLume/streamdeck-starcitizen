# /// script
# dependencies = ["lxml"]
#
# [tool.uv]
# exclude-newer = "2025-06-01T00:00:00Z"
# ///

"""
Find toggle action groups in Star Citizen's defaultProfile.xml.

Uses two reliable XML signals to identify real toggle actions:
  1. <states> child elements (explicit on/off, locked/unlocked, etc.)
  2. activationMode="smart_toggle"

Then finds enable/disable siblings via 100% token overlap within the same
actionmap.

Usage:
    uv run scripts/find-toggle-groups.py <path-to-defaultProfile.xml> [output-file]

If output-file is omitted, writes to scripts/toggle-groups.txt.
"""

import sys
from dataclasses import dataclass, field
from pathlib import Path
from lxml import etree


# ── Data structures ──────────────────────────────────────────────────────────────


@dataclass
class ActionState:
    name: str
    ui_label: str


@dataclass
class Action:
    name: str
    activation_mode: str
    ui_label: str
    states: list[ActionState] = field(default_factory=list)

    @property
    def is_toggle(self) -> bool:
        """True if the XML marks this action as a real toggle."""
        return bool(self.states) or self.activation_mode == "smart_toggle"


@dataclass
class ToggleGroup:
    map_name: str
    toggle: Action
    siblings: list[tuple[Action, float]]  # (action, overlap_score)


# ── Parsing ──────────────────────────────────────────────────────────────────────


def extract_actions(xml_path: str) -> dict[str, list[Action]]:
    """Parse defaultProfile.xml → {actionmap_name: [Action, ...]}."""
    tree = etree.parse(xml_path)
    result: dict[str, list[Action]] = {}

    for amap in tree.iter("actionmap"):
        map_name = amap.get("name", "")
        actions: list[Action] = []

        for act_el in amap.iter("action"):
            name = act_el.get("name", "")
            if not name:
                continue

            activation_mode = act_el.get("activationMode", "")
            ui_label = act_el.get("UILabel", "")

            states: list[ActionState] = []
            states_el = act_el.find("states")
            if states_el is not None:
                for state_el in states_el.findall("state"):
                    states.append(ActionState(
                        name=state_el.get("name", ""),
                        ui_label=state_el.get("UILabel", ""),
                    ))

            actions.append(Action(
                name=name,
                activation_mode=activation_mode,
                ui_label=ui_label,
                states=states,
            ))

        if actions:
            result[map_name] = actions

    return result


# ── Tokenization & matching ──────────────────────────────────────────────────────

## Noise words stripped during tokenization.  Only strip "role-neutral" words
## (the toggle/set/enable/disable prefixes).  Keep open/close/lock/unlock etc.
## because they distinguish WHICH group a sibling belongs to.
NOISE = {"v", "p", "toggle", "enable", "disable", "set", "start", "stop",
         "short", "long"}


def tokenize(name: str) -> set[str]:
    """Split an action name into meaningful tokens, dropping noise words."""
    parts = name.lower().replace("-", "_").split("_")
    return {p for p in parts if len(p) > 1 and p not in NOISE}


def stem_state(name: str) -> str:
    """Rough stem of a state name: opened→open, closed→close, etc."""
    s = name.lower()
    # retracted → retract, deployed → deploy, opened → open, etc.
    if s.endswith("ed") and len(s) > 4:
        # Handle double consonant: "equipped" → "equip" (strip "ped")
        base = s[:-2]
        if base.endswith("pp"):
            base = base[:-1]
        return base
    return s


def state_keywords(toggle: Action) -> set[str] | None:
    """Derive role keywords from a toggle's <states>.

    Returns a set of stemmed state names (e.g. {"open", "close"}) that
    siblings must contain, or None if no states are available.
    """
    if not toggle.states:
        return None
    keywords: set[str] = set()
    for st in toggle.states:
        stemmed = stem_state(st.name)
        if stemmed not in ("on", "off"):
            keywords.add(stemmed)
    # If all states are just on/off, return None to fall back to token matching
    return keywords if keywords else None


def find_siblings(toggle: Action, all_actions: list[Action]) -> list[tuple[Action, float]]:
    """Find non-toggle actions that are enable/disable variants of this toggle.

    Matching strategy:
    - If the toggle has <states> with descriptive names (not just on/off),
      a sibling must contain a stemmed state name AND have >=50% token overlap.
      This handles compound-word mismatches like doorlocks vs doors+lock.
    - Otherwise, requires all toggle tokens to appear in the sibling (subset).
    """
    toggle_tokens = tokenize(toggle.name)
    if not toggle_tokens:
        return []

    keywords = state_keywords(toggle)

    siblings: list[tuple[Action, float]] = []
    for other in all_actions:
        if other.name == toggle.name:
            continue
        if other.is_toggle:
            continue
        other_tokens = tokenize(other.name)
        if not other_tokens:
            continue

        if keywords:
            # State-keyword path: sibling must contain a state keyword AND
            # share at least half the toggle's tokens (handles compound words)
            other_lower = other.name.lower()
            has_keyword = any(kw in other_lower for kw in keywords)
            overlap = toggle_tokens & other_tokens
            enough_overlap = len(overlap) >= len(toggle_tokens) * 0.5
            if not (has_keyword and enough_overlap):
                continue
        else:
            # Fallback: strict subset match
            if not toggle_tokens.issubset(other_tokens):
                continue

        overlap = toggle_tokens & other_tokens
        score = len(overlap) / len(toggle_tokens | other_tokens)
        siblings.append((other, score))

    siblings.sort(key=lambda x: (-x[1], x[0].name))
    return siblings


## Baseline suffixes stripped from on/off action names to derive the "base"
## for reverse-searching a toggle candidate.  Extended at runtime with stemmed
## state names from all known toggles (so we automatically learn words like
## "deploy", "retract", "lock", "unlock", etc. from the XML).
_BASE_ON_OFF_SUFFIXES = {"on", "off", "enable", "disable"}


def build_on_off_suffixes(all_groups: list[ToggleGroup]) -> set[str]:
    """Extend the baseline suffix set with stemmed state names from all toggles."""
    suffixes = set(_BASE_ON_OFF_SUFFIXES)
    for g in all_groups:
        for st in g.toggle.states:
            suffixes.add(stem_state(st.name))
    return suffixes


def find_orphan_toggle(
    on_action: Action,
    off_action: Action,
    all_actions: list[Action],
    known_toggles: set[str],
    on_off_suffixes: set[str],
) -> Action | None:
    """Reverse-search for a toggle action that matches an on/off pair.

    Looks for an action in the same map whose tokens match the base tokens
    of the on/off pair (i.e. the name without the on/off/deploy/etc suffix).
    Candidates with "toggle" or "cycle" in their name are preferred.
    """
    base_tokens = tokenize(on_action.name) - on_off_suffixes
    if not base_tokens:
        return None

    best: Action | None = None
    best_score = 0.0
    for candidate in all_actions:
        if candidate.name in known_toggles:
            continue
        if candidate.name == on_action.name or candidate.name == off_action.name:
            continue
        cand_tokens = tokenize(candidate.name)
        if not cand_tokens:
            continue

        # Candidate's tokens should match the base tokens
        if not (cand_tokens == base_tokens
                or (cand_tokens.issubset(base_tokens) and len(cand_tokens) >= len(base_tokens))):
            # Also allow candidates with one extra token like "toggle" or "cycle"
            extra = cand_tokens - base_tokens
            non_noise = extra - {"cycle"}  # "toggle" already stripped by tokenize
            if non_noise or not extra:
                continue
            # Has "cycle" as only extra token — acceptable
            if not base_tokens.issubset(cand_tokens):
                continue

        overlap = cand_tokens & base_tokens
        score = len(overlap) / len(cand_tokens | base_tokens)

        # Prefer candidates with "toggle" or "cycle" in original name
        has_toggle_hint = "toggle" in candidate.name.lower() or "cycle" in candidate.name.lower()
        adjusted = score + (0.1 if has_toggle_hint else 0.0)

        if adjusted > best_score:
            best_score = adjusted
            best = candidate

    return best


# ── Group discovery ──────────────────────────────────────────────────────────────


def find_toggle_groups(actions_by_map: dict[str, list[Action]]) -> list[ToggleGroup]:
    """Find all real toggle actions and their enable/disable siblings."""
    groups: list[ToggleGroup] = []

    for map_name, actions in actions_by_map.items():
        for action in actions:
            if not action.is_toggle:
                continue
            siblings = find_siblings(action, actions)
            groups.append(ToggleGroup(
                map_name=map_name,
                toggle=action,
                siblings=siblings,
            ))

    return groups


# ── Report formatting ────────────────────────────────────────────────────────────


def format_report(actions_by_map: dict[str, list[Action]], groups: list[ToggleGroup]) -> str:
    lines: list[str] = []
    total_actions = sum(len(v) for v in actions_by_map.values())
    total_toggles = sum(1 for acts in actions_by_map.values() for a in acts if a.is_toggle)

    lines.append(f"Parsed {len(actions_by_map)} actionmaps, {total_actions} total actions")
    lines.append(f"Found {total_toggles} real toggle actions (have <states> or activationMode=smart_toggle)")
    lines.append("")

    # ── Groups with siblings ─────────────────────────────────────────────────
    with_siblings = [g for g in groups if g.siblings]
    lines.append("=" * 80)
    lines.append(f"TOGGLE GROUPS WITH SIBLINGS ({len(with_siblings)})")
    lines.append("=" * 80)
    lines.append("")

    for g in with_siblings:
        signal = "states" if g.toggle.states else "smart_toggle"
        state_names = ", ".join(s.name for s in g.toggle.states) if g.toggle.states else ""
        lines.append(f"[{g.map_name}] {g.toggle.name}  ({signal}{': ' + state_names if state_names else ''})")
        for sib, score in g.siblings:
            lines.append(f"    {score:.0%} {sib.name}")
        lines.append("")

    # ── Toggles without siblings ─────────────────────────────────────────────
    without = [g for g in groups if not g.siblings]
    lines.append("=" * 80)
    lines.append(f"TOGGLE ACTIONS WITHOUT SIBLINGS ({len(without)})")
    lines.append("  (real toggles but no enable/disable actions found via 100% token match)")
    lines.append("=" * 80)
    lines.append("")

    for g in without:
        signal = "states" if g.toggle.states else "smart_toggle"
        state_names = ", ".join(s.name for s in g.toggle.states) if g.toggle.states else ""
        lines.append(f"[{g.map_name}] {g.toggle.name}  ({signal}{': ' + state_names if state_names else ''})")

    lines.append("")

    # ── Non-toggle actions that look like on/off but have no toggle ──────────
    lines.append("=" * 80)
    lines.append("ORPHAN ON/OFF PAIRS (on/off actions without a matching toggle)")
    lines.append("=" * 80)
    lines.append("")

    on_off_suffixes = build_on_off_suffixes(groups)
    lines.append(f"  (on/off suffixes derived from states: {sorted(on_off_suffixes)})")
    lines.append("")

    known_toggle_names: set[str] = {g.toggle.name for g in groups}
    # Also track which on/off actions are already claimed as siblings
    claimed: set[str] = set()
    for g in groups:
        for sib, _ in g.siblings:
            claimed.add(sib.name)

    # Build suffix lists for detecting on/off actions from the derived set
    on_suffixes = sorted(s for s in on_off_suffixes
                         if s in {"on", "enable", "open", "unlock", "deploy",
                                  "start", "equip", "attach"} or s in on_off_suffixes)
    off_suffixes = sorted(s for s in on_off_suffixes
                          if s in {"off", "disable", "close", "lock", "retract",
                                   "stop"} or s in on_off_suffixes)

    def looks_like_on(a: Action) -> bool:
        low = a.name.lower()
        return any(low.endswith("_" + s) or ("_" + s + "_") in low
                   for s in ("on", "enable", "open", "unlock", "deploy"))

    def looks_like_off(a: Action) -> bool:
        low = a.name.lower()
        return any(low.endswith("_" + s) or ("_" + s + "_") in low
                   for s in ("off", "disable", "close", "lock", "retract"))

    for map_name, actions in sorted(actions_by_map.items()):
        on_actions = [a for a in actions if not a.is_toggle and a.name not in claimed
                      and looks_like_on(a)]
        off_actions = [a for a in actions if not a.is_toggle and a.name not in claimed
                       and looks_like_off(a)]

        if not on_actions:
            continue

        for on_a in on_actions:
            on_tokens = tokenize(on_a.name)
            # Find best matching off action
            best_off: Action | None = None
            best_score = 0.0
            for off_a in off_actions:
                off_tokens = tokenize(off_a.name)
                overlap = on_tokens & off_tokens
                if overlap:
                    score = len(overlap) / len(on_tokens | off_tokens)
                    if score > best_score:
                        best_score = score
                        best_off = off_a

            if not best_off:
                continue

            # Try to find a hidden toggle for this pair
            toggle_candidate = find_orphan_toggle(
                on_a, best_off, actions, known_toggle_names, on_off_suffixes,
            )

            lines.append(f"[{map_name}]")
            if toggle_candidate:
                lines.append(f"  TOGGLE: {toggle_candidate.name}  (hidden — no <states> or smart_toggle)")
            lines.append(f"  ON:  {on_a.name}")
            lines.append(f"  OFF: {best_off.name}")
            lines.append("")

    return "\n".join(lines)


# ── Main ─────────────────────────────────────────────────────────────────────────


def main():
    if len(sys.argv) < 2:
        print("Usage: uv run scripts/find-toggle-groups.py <defaultProfile.xml> [output-file]")
        sys.exit(1)

    xml_path = sys.argv[1]
    default_out = Path(__file__).parent / "toggle-groups.txt"
    out_path = Path(sys.argv[2]) if len(sys.argv) >= 3 else default_out

    actions_by_map = extract_actions(xml_path)
    groups = find_toggle_groups(actions_by_map)
    report = format_report(actions_by_map, groups)

    out_path.write_text(report, encoding="utf-8")
    print(f"Wrote {out_path} ({len(groups)} toggle groups)")


if __name__ == "__main__":
    main()
