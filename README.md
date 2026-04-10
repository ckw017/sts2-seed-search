```
cargo build --release
.\target\release\seedfinder.exe --count 1000000 --doll-pos 1 --reflect-pos 5  --max-shops 3 --drowning-pos 4 --hopper-second --threads 14 | Tee-Object -FilePath results.txt
```

Update --threads to however many cores you want to use

If you want skip a block of seeds you've already checked, use `--start-seed <number to start at>`
