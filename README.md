```
cargo build --release
.\target\release\seedfinder.exe --count 20 --doll-pos 1 --reflect-pos 5  --max-shops 3 --drowning-pos 4 --hopper-second --threads 14 | Tee-Object -FilePath results.txt 
```

(update --threads to however many cores you want to use)
