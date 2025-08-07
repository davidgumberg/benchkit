# Todo

## General

- [x] `benchkit run out_dir`
-  out_dir:
    -  ci_run_id
        - config.yml
        - benchmark.yml
        -  run_command:
            -  hyperfine_run_id: # There is no such thing...
                - debug.log
                - perf.data [optional]
        - result.json

- [ ] Could be nice to add a list of files to be copied to out_dir to benchmark.config so it's dynamic
- [ ] Need to also account for hyperfine iterations, see https://github.com/sharkdp/hyperfine/pull/807
- [x] remove PR number
- [x] remove branch


## CI workflow
- [ ] run benchkit
- [ ] upload out_dir/ci_run_id to S3 bucket
- [ ] Process out_dir/ci_run_id
    - commit result of process out_dir to gh_pages tree
- [ ] out_dir/ci_run_id wiped from runner

## Database
- [x] Decouple db so that it's not required for `run`.
    - [x] Remove requirement
    - [x] Remove requirement from config file
- [ ] Make `benchkit db upload out_dir` add to db

## S3
- [ ] prob make upload command like db

## Assumeutxo patching
- [x] We should fetch patches dynamically from a repo (more up-to-date)
- [ ] Also enable dynamic merge conflict resolution instead of panic!

## Snapshot
- [ ] Add snapshot config option? (may not yet)
- [ ] and stopatheight by default?

## Full IDB bench
- [ ] Should this be easier?
    - Currently to skip snapshot load amend the `prepare:` config script to a NO_OP, e.g. `echo don't prepare for IBD`

## Perf
- [ ] how do?

## What is "done"?
- [ ] A final solution for the assumeutxo patching
- [ ] Nightly regression testing runs
- [ ] Ability for users to run a random PR on demand

## Future
### Hardware / runner_profiles
- [ ] We have a way of describing a machine hardware spec
    - [x] Added a system_info dump to out_dir
    - [ ] Should we include more than this?

## AMS

- Bake in the various setup scripts into benchkit binary
- Create an instrumented build script
- Make sure everything stays seperate
- In terms of prior state:
  - Assumutxo (done)
  - data dir (for migrations)
  - clean (no prior state)

Static box with nightly checkout of repo
