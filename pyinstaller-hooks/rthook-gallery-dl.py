# Preload gallery-dl’s extractor whose filename isn't a valid Python identifier ("2ch.py")
# so that dynamic discovery can import it under the name "gallery_dl.extractor.2ch".
import os, sys, tempfile, pkgutil, importlib.util

def _preload_extractor(modname: str, relpath: str):
    data = pkgutil.get_data('gallery_dl.extractor', relpath)
    if not data:
        return  # not found in the bundle; ignore
    tmpdir = os.path.join(tempfile.gettempdir(), 'gallery_dl_pyinst_extractors')
    os.makedirs(tmpdir, exist_ok=True)
    dst = os.path.join(tmpdir, relpath)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, 'wb') as f:
        f.write(data)
    spec = importlib.util.spec_from_file_location(modname, dst)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    sys.modules[modname] = module

# This is the one that blows up in your logs.
_preload_extractor('gallery_dl.extractor.2ch', '2ch.py')
