#!/usr/bin/env python3
"""Plot Rust-generated spectra and retained inference outputs; no signal processing."""
import argparse,json,pathlib
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--spectrum',type=pathlib.Path,required=True);a=p.parse_args();s=json.loads(a.spectrum.read_text())['spectrogram'];t=np.array(s['times_s']);f=s['min_hz']+np.arange(s['bins'])*s['bin_hz'];power=np.array(s['power_db']).reshape(len(t),s['bins'])
 old=json.loads((ROOT/'validation-runs/locations/colsto-high-recall-discovery.json').read_text())['analysis'];new=json.loads((ROOT/'validation-runs/improvement/colsto-final.json').read_text())['analysis'];fig,axes=plt.subplots(2,2,figsize=(13,7),sharey=True)
 for row,(lo,hi) in enumerate([(0,120),(925,1045)]):
  select=(t>=lo)&(t<=hi)
  for col,res in enumerate([old,new]):
   ax=axes[row,col];ax.imshow(power[select].T,origin='lower',extent=[t[select][0],t[select][-1],f[0],f[-1]],aspect='auto',vmin=0,vmax=22,cmap='magma');ax.set_xlim(lo,hi);ax.set_ylim(70,450);ax.set_xlabel('Seconds from continuous excerpt start')
   if col==0:
    for e in res['events']:ax.plot([p['time_s'] for p in e['points']],[p['frequency_hz'] for p in e['points']],c='#39d5e5',lw=1)
   else:
    for m in res['candidates'][0]['matches']:
     e=res['events'][m['event_id']];ax.plot([p['time_s'] for p in e['points']],[p['frequency_hz'] for p in e['points']],c='#39d5e5',lw=1)
     ax.plot([p['time_s'] for p in m['predicted']],[p['frequency_hz'] for p in m['predicted']],c='#fff27a',lw=1.3)
   if row==0:ax.set_title(['Earlier greedy proposals','Revised selected paths + retained predicted supports'][col])
   if col==0:ax.set_ylabel('Frequency / Hz')
 fig.suptitle('Original recording is development material; selected paths are not ground-truth detector labels',fontsize=11);fig.tight_layout();fig.savefig(ROOT/'docs/algorithm-diagnosis.png',dpi=160);plt.close(fig)
 proto=json.loads((ROOT/'data/independent-validation/pathak-protocol.json').read_text());emb=proto['arbitrary_embedding'];rx,ry=proto['reference']['reference_xy_m'];fig,axes=plt.subplots(1,2,figsize=(11,5),sharex=True,sharey=True)
 for ax,engine in zip(axes,['baseline','revised']):
  a=json.loads((ROOT/('validation-runs/independent/site.'+engine+'.json')).read_text())['fusion'];h=a['heatmap'];top=max(c['score'] for c in h);x=[c['lon']*emb['east_m_per_degree']/1000 for c in h];y=[c['lat']*emb['north_m_per_degree']/1000 for c in h];im=ax.scatter(x,y,c=[c['score']-top for c in h],cmap='viridis',vmin=-6,vmax=0,marker='s',s=105)
  for i,c in enumerate(a['candidates'][:4]):
   xx=c['lon']*emb['east_m_per_degree']/1000;yy=c['lat']*emb['north_m_per_degree']/1000;ax.scatter(xx,yy,marker='x' if i==0 else 'o',s=85,facecolors='none' if i else 'black',edgecolors='black');ax.annotate(str(i+1),(xx+.1,yy+.1),fontsize=9)
  ax.scatter(rx/1000,ry/1000,c='red',edgecolors='white',s=180,marker='*',label='Metadata reference');ax.set_aspect('equal');ax.set_xlim(-5,5);ax.set_ylim(-5,5);ax.set_title(engine.capitalize()+' · all five separate recordings');ax.set_xlabel('East from town prior / km');ax.legend(loc='upper left',fontsize=8)
 axes[0].set_ylabel('North from town prior / km');fig.suptitle('Equal-weight shared-site scores; independent clocks; reference excluded from inference',fontsize=11);fig.tight_layout();fig.savefig(ROOT/'docs/independent-site.png',dpi=160);plt.close(fig)
if __name__=='__main__':main()
