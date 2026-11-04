#!/usr/bin/env python3
"""Metadata-only reference reconstruction and finite data preparation. No audio DSP."""
import csv,datetime,hashlib,json,math,pathlib,statistics
ROOT=pathlib.Path(__file__).resolve().parents[1]
OUT=ROOT/'data/independent-validation';OUT.mkdir(exist_ok=True)
LAT0,LON0=37.68,-121.77
SX,SY=111320*math.cos(math.radians(LAT0)),111000.
EAST,NORTH=111319.49079327358,110574.27582159435

def unix(s):return datetime.datetime.fromisoformat(s.replace('Z','+00:00')).timestamp()
def rows(folder):return list(csv.DictReader((folder/'track.normalized.csv').open()))
def local(r):return ((float(r['lon'])-LON0)*SX,(float(r['lat'])-LAT0)*SY)
def closest(x,y,track):
 best=None
 for a,b in zip(track,track[1:]):
  if a['segment_index']!=b['segment_index']:continue
  ax,ay=local(a);bx,by=local(b);dx,dy=bx-ax,by-ay
  u=max(0,min(1,((x-ax)*dx+(y-ay)*dy)/max(1e-12,dx*dx+dy*dy)))
  d=math.hypot(x-ax-u*dx,y-ay-u*dy)
  z=float(a['height_egm96_msl_m'])+u*(float(b['height_egm96_msl_m'])-float(a['height_egm96_msl_m']))
  if best is None or d<best[0]:best=(d,z)
 return best

def main():
 old=json.loads((ROOT/'corpus/pathak/manifest.json').read_text());constraints=[]
 for case in old['cases']:
  t=unix(case['first_ridge_utc']);track=[r for r in rows(ROOT/'corpus/pathak'/case['id']) if t-90<float(r['time_unix_s'])<t+180]
  constraints.append((case,track,float(case['author_summary']['L_ADSB_m'])))
 def residual(x,y):return [closest(x,y,r)[0]-l for _,r,l in constraints]
 solutions=[]
 for x0 in [-3000.,0.,3000.]:
  for y0 in [-3000.,0.,3000.]:
   x,y=x0,y0
   for _ in range(100):
    rr=residual(x,y);jx=[(p-q)/.02 for p,q in zip(residual(x+.01,y),residual(x-.01,y))];jy=[(p-q)/.02 for p,q in zip(residual(x,y+.01),residual(x,y-.01))]
    aa=sum(a*a for a in jx)+1e-9;bb=sum(a*b for a,b in zip(jx,jy));dd=sum(b*b for b in jy)+1e-9;rx=-sum(a*r for a,r in zip(jx,rr));ry=-sum(b*r for b,r in zip(jy,rr));det=aa*dd-bb*bb
    dx=(rx*dd-ry*bb)/det;dy=(ry*aa-rx*bb)/det;scale=min(1,3000/max(1,math.hypot(dx,dy)));x+=scale*dx;y+=scale*dy
    if math.hypot(dx,dy)<1e-7:break
   solutions.append((sum(r*r for r in residual(x,y)),x,y))
 _,rx,ry=min(solutions);heights=[closest(rx,ry,tr)[1]-float(c['author_summary']['H_ADSB_above_recorder_m']) for c,tr,_ in constraints];height=statistics.median(heights)
 reference={'kind':'metadata-reconstructed reference; not independently surveyed GPS','audio_used':False,'reference_lat':LAT0+ry/SY,'reference_lon':LON0+rx/SX,'reference_xy_m':[rx,ry],'recorder_msl_m':height,'source_cases':[c['id'] for c,_,_ in constraints],'offset_residuals_m':residual(rx,ry),'height_estimates_msl_m':heights,'uncertainty_bound_known':False,'conditional_on':'The source used one recorder site for these passages; nearby-point geometric consistency is checked separately for every new case.'}
 (OUT/'reference.json').write_text(json.dumps(reference,indent=2)+'\n')
 fresh=json.loads((ROOT/'corpus/pathak-heldout/manifest.json').read_text());all_cases=[(ROOT/'corpus/pathak',c) for c in old['cases']]+[(ROOT/'corpus/pathak-heldout',c) for c in fresh['cases']]
 bounds={'west':-5000/EAST,'east':5000/EAST,'south':-5000/NORTH,'north':5000/NORTH};cases=[]
 for case in fresh['cases']:
  id=case['id'];duration=case['audio']['duration_seconds'];t=unix(case['first_ridge_utc']);flights=[]
  for base,c in all_cases:
   seg=[];index=0
   def flush():
    nonlocal seg,index
    if len(seg)>=2:flights.append({'id':c['id']+'-'+str(index),'aircraft_type':'source aircraft','points':seg})
    seg=[];index+=1
   for r in rows(base/c['id']):
    at=float(r['time_unix_s']);x,y=local(r)
    if not t-duration-120<=at<=t+duration+120 or math.hypot(x,y)>25000:continue
    if seg and (at-seg[-1]['time_unix_s']>30 or at<=seg[-1]['time_unix_s']):flush()
    seg.append({'time_unix_s':at,'lat':y/NORTH,'lon':x/EAST,'height_ellipsoid_m':float(r['height_egm96_msl_m'])-height})
   flush()
  pack={'schema_version':1,'name':'Pathak separate recording '+id,'source':'Pathak 10.5281/zenodo.23424278; supplied source tracks; relative EGM96 heights embedded as local ENU at arbitrary WGS84(0,0). No geometric altitude relabeling.','license':'CC-BY-4.0','coordinate_frame':'local_cartesian_embedded','clock_domain':'utc','synthetic':False,'search_bounds':bounds,'coverage_start_unix_s':min(f['points'][0]['time_unix_s'] for f in flights),'coverage_end_unix_s':max(f['points'][-1]['time_unix_s'] for f in flights),'coverage_notes':['Supplied-track validation, not complete competing traffic.','10 km square centered on a declared town prior, not receiver truth. All source KMLs available to this protocol were considered.','Local x/y are equirectangular offsets from Livermore town prior; z is source EGM96 height relative to the metadata-derived recorder height. Origin is arbitrary; do not interpret embedded coordinates as Earth latitude/longitude.','Recording start is uncertain within a metadata-derived interval; no ridge curve is passed to inference.'],'flights':flights}
  (OUT/(id+'.package.json')).write_text(json.dumps(pack,separators=(',',':'))+'\n')
  settings={'max_audio_s':1080,'min_hz':35,'max_hz':1000,'window_s':2,'hop_s':.5,'min_event_s':8,'peak_threshold_db':9,'max_events':16,'multiresolution':False,'frequency_sigma_hz':2,'grid_spacing_m':500,'mode_separation_m':300,'refinement_levels':0,'max_cells':5000,'max_hypotheses':2000000,'max_track_gap_s':30,'clock_step_s':2,'clock_radius_s':20,'observer_height_ellipsoid_m':0,'time_prior':{'windows':[{'start_unix_s':t-duration,'end_unix_s':t+duration,'weight':1}]}}
  (OUT/(id+'.baseline.config.json')).write_text(json.dumps(settings,indent=2)+'\n');settings.update(coherent_ridges=True,robust_shape=True)
  (OUT/(id+'.revised.config.json')).write_text(json.dumps(settings,indent=2)+'\n')
  track=[r for r in rows(ROOT/'corpus/pathak-heldout'/id) if t-90<float(r['time_unix_s'])<t+duration+120];offset=closest(rx,ry,track)[0]
  cases.append({'id':id,'audio':str((ROOT/'corpus/pathak-heldout'/id/'audio.wav').relative_to(ROOT)),'expected_flight_prefix':id,'reference_offset_check_m':offset,'source_offset_m':float(case['author_summary']['L_ADSB_m']),'reference_offset_residual_m':offset-float(case['author_summary']['L_ADSB_m']),'interpretation':'Single pass with uncertain clock has along-track/time and mirror ambiguities. Site fusion uses all five independent recordings with separate clocks and equal weights. No failed case is silently dropped.'})
 protocol={'reference':reference,'arbitrary_embedding':{'east_m_per_degree':EAST,'north_m_per_degree':NORTH,'town_lat':LAT0,'town_lon':LON0,'east_source_scale':SX,'north_source_scale':SY},'cases':cases,'not_a_blind_aircraft_identification_test':True,'fusion_predeclared':'sum the per-cell profiled scores, independently profiling each recording clock; require evidence from all five recordings; same coarse 500 m grid, no truth-dependent weights or basin choice; no calibrated probabilities'}
 (OUT/'pathak-protocol.json').write_text(json.dumps(protocol,indent=2)+'\n');print(json.dumps({'reference':reference,'new_metadata_offset_checks':[{k:c[k] for k in ['id','reference_offset_residual_m']} for c in cases]},indent=2))
if __name__=='__main__':main()
