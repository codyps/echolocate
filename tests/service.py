#!/usr/bin/env python3
"""Exercise the real native HTTP bridge, persistent jobs, errors and cancellation."""
import json, os, pathlib, socket, struct, subprocess, tempfile, time, urllib.error, urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    base=f'http://127.0.0.1:{port}'
    def get(path,body=None,origin=None):
        headers={'Content-Type':'application/octet-stream'}
        if origin:headers['Origin']=origin
        request=urllib.request.Request(base+path,data=body,headers=headers)
        with urllib.request.urlopen(request,timeout=30) as r:return r.status,json.load(r)
    def wait(id):
        for _ in range(200):
            _,state=get('/api/jobs/'+id)
            if state['state'] in ('complete','failed','cancelled'):return state
            time.sleep(.05)
        raise RuntimeError('Job timed out')
    with tempfile.TemporaryDirectory(prefix='echo-service-') as directory:
        binary=os.environ.get('ECHO_BINARY',str(ROOT/'target/release/echo-locate'))
        proc=subprocess.Popen([binary,'serve','--port',str(port),'--web',str(ROOT/'dist'),'--jobs',directory],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        try:
            for _ in range(100):
                try:
                    if get('/api/health')[1]['ok']:break
                except Exception:time.sleep(.05)
            else:raise RuntimeError('Service failed to start')
            request={'config':json.loads((ROOT/'web/demo.config.json').read_text()),'package':json.loads((ROOT/'web/demo.package.json').read_text())}
            metadata=json.dumps(request).encode();body=struct.pack('<I',len(metadata))+metadata+(ROOT/'web/demo.wav').read_bytes()
            code,started=get('/api/jobs',body);assert code==202;id=started['id'];assert wait(id)['state']=='complete'
            _,result=get('/api/jobs/'+id+'/result');assert result['ok'];assert result['analysis']['candidates'][0]['matched_events']==2
            assert (pathlib.Path(directory)/id/'request.json').exists();assert (pathlib.Path(directory)/id/'audio.bin').exists()
            try:get('/api/jobs',b'bad')
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Malformed bridge body accepted')
            try:get('/api/jobs',body,'https://example.com')
            except urllib.error.HTTPError as e:assert e.code==403
            else:raise AssertionError('Cross-origin write accepted')
            _,started=get('/api/jobs',body);id=started['id'];get('/api/jobs/'+id+'/cancel',b'');assert wait(id)['state']=='cancelled'
            assert json.loads((pathlib.Path(directory)/id/'result.json').read_text())['ok'] is False
            print('PASS: native job inference, saved inputs/result, invalid input, origin check, cooperative cancellation')
        finally:
            proc.terminate();proc.wait(timeout=10)
if __name__=='__main__':main()
