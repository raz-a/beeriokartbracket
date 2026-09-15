# OBS companion

This standalone executable polls the public tournament snapshot every two
seconds and atomically writes plain-text files for OBS text sources. It does not
need the private GUI publishing token.

Run `beeriokartbracket-obs-companion.exe`, then point OBS text sources at files
in the `obs-output` directory beside the executable:

- `tournament_name.txt`
- `phase.txt`
- `heat.txt`
- `ruleset.txt`
- `active_racers.txt` (all active racers, one per line)
- `racer_1.txt` through `racer_8.txt`
- `placement_1.txt` through `placement_8.txt`

Inactive race files are emptied so stale names disappear from OBS. At tournament
completion, the racer and placement files contain final results in finishing
order.

## Configuration

Defaults work with the production tournament feed. To override them, place an
`obs-companion.json` file beside the executable:

```json
{
	"snapshot_url": "https://beeriokartbracket-api.beeriokart.workers.dev/snapshot",
	"output_directory": "obs-output",
	"poll_interval_ms": 2000
}
```

Relative output directories are resolved from the executable directory. The
minimum poll interval is 250 milliseconds.