
import os
from pathlib import Path
original = os.replace
def replace(src, dst, *args, **kwargs):
    destinations = ['id14-sr', 'libid14_sr_ladspa.so', 'id14-sr-filter.service',
                    'lifecycle.py', 'id14-sr-restore.service']
    point = os.environ.get('FAIL_POINT', '')
    marker = Path(os.environ['INJECTED'])
    hit = str(src).endswith('.id14-sr-new') and Path(dst).name in destinations
    if hit and not marker.exists() and point in ('replace-before:' + Path(dst).name, 'replace-after:' + Path(dst).name):
        marker.touch()
        if point.startswith('replace-after:'):
            original(src, dst, *args, **kwargs)
        if os.environ.get('FAIL_SIGNAL'):
            os.kill(os.getpid(), int(os.environ['FAIL_SIGNAL']))
        raise OSError('injected publication failure')
    return original(src, dst, *args, **kwargs)
os.replace = replace
original_chmod = os.chmod
def chmod(path, *args, **kwargs):
    marker = Path(os.environ['INJECTED'])
    if os.environ.get('FAIL_POINT') == 'stage-mode:' + Path(path).name and not marker.exists():
        marker.touch()
        raise OSError('injected staging failure')
    return original_chmod(path, *args, **kwargs)
os.chmod = chmod
