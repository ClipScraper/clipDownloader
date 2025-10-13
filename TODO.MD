- handle errors
- get collections from extension
- add tests
- option to cap file name size
- complete sidebar

- create a button to put next to DL button in /home | Queue button, to put a download for later
- rename greet var from home

- Cancel download
- preview next download
- parallel downloads
- button to prioritize in backlog
- sanitize logs
- revert button to put back from queue to backlog
- make sure that clicking trash removes from library
- split db into files (wtf did I mean there -_-')
- add play & trash button in library
- add settings option to download mp3 or mp4

settings: 
- button:
    - allow db duplicate / replace db duplicaet

make sure image/videos have same settings code

#################################### Bug:
- Using download url on a playlist like: https://www.youtube.com/watch?v=wD0Mm6WIcYs&list=RD0vb1CtGkqtw&index=3 downloads the entire playlist without adding the videos in the database
- Cancel to interrupt DL of videos doesn't work, it kept downloading the full playlist even after clicking Cancel
- instagram name doesn't match exactly: eg https://www.instagram.com/p/DEZ5PboSW-l/ -> CuteyTaoTao🥰❤️🤩 [DEZ5PboSW-l].mp4 instead of dudunatao
- keeps downlaod going even after leaving /download page

BUG youtube:
2025-10-13T05:10:25.707690Z  INFO frontend: 15: download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.766559Z  INFO frontend: 15: queue_autostart context={"url":"https://www.youtube.com/watch?v=NgZJyu_Qk7Y&list=WL&index=325&pp=gAQBiAQB0gcJCesJAYcqIYzv"}
2025-10-13T05:10:25.768973Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.769019Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.769138Z  INFO frontend: 15: download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.827024Z  INFO frontend: 15: queue_autostart context={"url":"https://www.youtube.com/watch?v=nH4qjmP2KEE&list=WL&index=201&pp=gAQBiAQB"}
2025-10-13T05:10:25.829256Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.829312Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.829376Z  INFO frontend: 15: download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.888053Z  INFO frontend: 15: queue_autostart context={"url":"https://www.youtube.com/watch?v=NIgrGqmoeHs&list=WL&index=26&pp=gAQBiAQB"}
2025-10-13T05:10:25.890906Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}
2025-10-13T05:10:25.890946Z  INFO frontend: 15: home_download_complete context={"message":"Failed to download with any available browser's cookies.","success":false}



- drag and drop doesn't work:
[BACKEND] [commands/files.rs] [read_csv_from_path] /Users/hjoncour/Downloads/0e93bac9-133d-44d0-b814-9502ebdaed9d.csv

-install yt dlp on windows
fix color buttons on windows


- Add "moved" to library section for files that were "done" but whose path changed
