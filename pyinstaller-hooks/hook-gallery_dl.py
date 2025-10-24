# Ensures gallery-dl’s dynamically-discovered modules are bundled in the frozen binary.
from PyInstaller.utils.hooks import collect_submodules, collect_data_files

hiddenimports = (
    collect_submodules("gallery_dl.extractor")
    + collect_submodules("gallery_dl.downloader")
    + collect_submodules("gallery_dl.postprocessor")
    + collect_submodules("gallery_dl.output")
)

datas = collect_data_files("gallery_dl", include_py_files=True)
