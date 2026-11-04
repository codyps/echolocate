#!/usr/bin/env python3
"""Prepare a fixed, declared local-frame protocol before reading receiver truth.

Only file/coordinate preparation is Python; detection and inference remain Rust.
Coordinates are embedded at an arbitrary WGS84 origin, NOT the flight's geographic location.
"""
import hashlib,json,math,pathlib,struct
ROOT=pathlib.Path(__file__).resolve().parents[1]
SOURCE=ROOT/'corpus/ground-truth/dlr-rec139'
OUT=ROOT/'data/location-validation/dlr'
BASE=1000.0
EAST_DEG=6378137*math.pi/180
NORTH_DEG=6335439.3272928195*math.pi/180
def save(path,value):path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(value,indent=2)+'\n')
def crop_wav(src,dest,start,end):
    raw=src.read_bytes();at=12;fmt=None;data=None
    while at+8<=len(raw):
        kind,size=struct.unpack_from('<4sI',raw,at);chunk=raw[at+8:at+8+size]
        if kind==b'fmt ':fmt=chunk
        elif kind==b'data':data=chunk
        at+=8+size+(size%2)
    code,channels,rate,_,align,bits=struct.unpack_from('<HHIIHH',fmt)
    assert code==3 and channels==1 and bits==32 and rate==48192
    a,b=round(start*rate),round(end*rate);samples=data[a*align:b*align]
    header=struct.pack('<4sI4s4sIHHIIHH4sI',b'RIFF',36+len(samples),b'WAVE',b'fmt ',16,code,channels,rate,rate*align,align,bits,b'data',len(samples))
    dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(header+samples)
    return a/rate,len(samples)//align/rate
def main():
    meta=json.loads((SOURCE/'metadata.json').read_text());source_manifest=json.loads((SOURCE/'manifest.json').read_text())
    states=[]
    for t in meta['trajectories'].values():states.append((t['reference_time_s_zero_based'],t['ReferencePosition']['value'],t['ReferenceVelocity']['value']))
    states.sort();c=meta['metadata']['speedOfSoundMPerS']['value'][0]
    wind=meta['trajectories']['Trajectory090']['WindVelocity']['value']
    cases=[];OUT.mkdir(parents=True,exist_ok=True)
    # Primary protocol: original supported knots, a common arrival slice valid over ±150 m.
    # Secondary protocol: explicit constant-velocity extension from the 90-degree reference.
    # Both are declared before running the algorithm or reading receiver-truth.json.
    for protocol in ['measured-knots','constant-velocity-extension']:
        half=150. if protocol=='measured-knots' else 500.
        bounds={'west':-half/EAST_DEG,'east':half/EAST_DEG,'south':-half/NORTH_DEG,'north':half/NORTH_DEG}
        source_states=[(t,p) for t,p,v in states]
        if protocol=='constant-velocity-extension':
            t,p,v=states[1];source_states=[(float(u),[p[k]+v[k]*(u-t) for k in range(3)]) for u in range(-5,13)]
        points=[{'time_unix_s':BASE+t,'lat':p[1]/NORTH_DEG,'lon':p[0]/EAST_DEG,'height_ellipsoid_m':p[2]} for t,p in source_states]
        package={'schema_version':1,'name':'DLR RECORD139 · local-frame '+protocol,'source':'Timo Schumacher; Henri Siller; DLR; https://zenodo.org/records/20494053','license':'CC-BY-4.0','coordinate_frame':'local_cartesian_embedded','clock_domain':'source_internal_seconds_plus_1000','coverage_start_unix_s':BASE-7.,'coverage_end_unix_s':BASE+14.,'search_bounds':bounds,'synthetic':False,'coverage_notes':['Real measured audio and receiver coordinates, but no absolute geography or UTC. Local x/y/z are embedded as east/north/up at arbitrary (0,0,0); output is not a geographic geolocation.','Zero-based reference samples interpreted as simultaneous source-state clock; source does not explicitly distinguish emission/reception convention. No alignment is fitted using receiver truth.','All channels are correlated observations of one flight. Uniform wind is supplied from the published 90-degree reference.','Source positions are original three measured knots; no source trajectory extrapolation.' if protocol=='measured-knots' else 'Source path is a modeled constant-velocity extension of published Trajectory090 state, not measured positions outside the reference.'],'flights':[{'id':'DLR-ATRA-REC139','aircraft_type':'A320','points':points}]}
        package['coverage_start_unix_s']=points[0]['time_unix_s'];package['coverage_end_unix_s']=points[-1]['time_unix_s']
        packpath=OUT/(protocol+'.package.json');save(packpath,package)
        for item in source_manifest['cases']:
            id=f"{item['id']}-{protocol}";audio=SOURCE/item['audio_path'];start=0.;duration=7.
            if protocol=='measured-knots':audio=OUT/(id+'.wav');start,duration=crop_wav(SOURCE/item['audio_path'],audio,3.,4.8)
            config={'min_hz':35.,'max_hz':6000.,'window_s':0.25,'hop_s':0.05,'multiresolution':False,'min_event_s':0.5,'min_drop_hz':2.,'max_events':8,'frequency_sigma_hz':4.,'bounds':bounds,'grid_spacing_m':25. if protocol=='measured-knots' else 50.,'mode_separation_m':25.,'refinement_levels':2,'max_cells':5000,'max_hypotheses':100000,'clock_step_s':0.1,'clock_radius_s':0.,'observer_height_ellipsoid_m':0.,'sound_speed_m_s':c,'wind_east_m_s':wind[0],'wind_north_m_s':wind[1],'wind_up_m_s':wind[2],'time_prior':{'windows':[{'start_unix_s':BASE+start-0.05,'end_unix_s':BASE+start+0.05,'weight':1.}]}}
            cfgpath=OUT/(id+'.config.json');save(cfgpath,config)
            cases.append({'id':id,'protocol':protocol,'audio':str(audio.relative_to(ROOT)),'package':str(packpath.relative_to(ROOT)),'config':str(cfgpath.relative_to(ROOT)),'source_channel':item['channel_zero_based'],'waveform_start_source_s':start,'duration_s':duration,'truth_file':'corpus/ground-truth/dlr-rec139/receiver-truth.json','truth_role':'evaluation only, never inference input','absolute_geographic_accuracy_eligible':False})
    save(OUT/'protocol.json',{'schema_version':1,'kind':'real_audio_local_frame_validation','arbitrary_embedding':{'origin_lat_lon_h':[0.,0.,0.],'east_m_per_degree':EAST_DEG,'north_m_per_degree':NORTH_DEG},'cases':cases,'notes':['No receiver coordinates read during preparation. Known geometry bounds enclose the array but are not individualized to any microphone.','Compare errors with the array-origin baseline; six correlated channels do not establish generalization.','No frequency band, event window, or clock is selected using location error. Primary crop is determined by shared source support and declared search extent.']})
    print('Prepared',len(cases),'real-audio cases in two explicit protocols')
if __name__=='__main__':main()
