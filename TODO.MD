PRIORITY:
---------
- button options mp4/mp3 for bulk download
- sync dl/library (lag in bulk dl)
---------
>>>>>>>>>>>>> have a version using local dependencies instead of sidecar to be faster
- implement use of local libraries instead

- handle errors
- get collections from extension
- add tests
- option to cap file name size
- create a button to put next to DL button in /home | Queue button, to put a download for later
- rename greet var from home

- Cancel download
- preview next download
- button to prioritize in backlog
- sanitize logs
- make sure that clicking trash removes from library
- add settings option to download mp3 or mp4

#################################### Bug:
- Using download url on a playlist like: https://www.youtube.com/watch?v=wD0Mm6WIcYs&list=RD0vb1CtGkqtw&index=3 downloads the entire playlist without adding the videos in the database
- Cancel to interrupt DL of videos doesn't work, it kept downloading the full playlist even after clicking Cancel
- instagram name doesn't match exactly: eg https://www.instagram.com/p/DEZ5PboSW-l/ -> CuteyTaoTao🥰❤️🤩 [DEZ5PboSW-l].mp4 instead of dudunatao

fix color buttons on windows

- Add "moved" to library section for files that were "done" but whose path changed
- Add option to save pictures/carousels in separate folders
