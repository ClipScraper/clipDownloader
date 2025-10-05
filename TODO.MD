- handle errors
- get collections from extension
- add tests
- option to cap file name size

- create a button to put next to DL button in /home | Queue button, to put a download for later
- rename greet var from home

- Cancel download
- preview next download
- parallel downloads
- refactor
- sanitize logs
- fix icon

settings: 
- button:
    - allow db duplicate / replace db duplicaet

make sure image/videos have same settings code

#################################### Bug:
- Using download url on a playlist like: https://www.youtube.com/watch?v=wD0Mm6WIcYs&list=RD0vb1CtGkqtw&index=3 downloads the entire playlist without adding the videos in the database
- Cancel to interrupt DL of videos doesn't work, it kept downloading the full playlist even after clicking Cancel
- instagram name doesn't match exactly: eg https://www.instagram.com/p/DEZ5PboSW-l/ -> CuteyTaoTao🥰❤️🤩 [DEZ5PboSW-l].mp4 instead of dudunatao

- drag and drop doesn't work:
[BACKEND] [commands/files.rs] [read_csv_from_path] /Users/hjoncour/Downloads/0e93bac9-133d-44d0-b814-9502ebdaed9d.csv

