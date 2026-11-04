#!/usr/bin/env python3
"""Acquire every unused constant-speed/height case, before acoustic evaluation."""
import importlib.util,json,pathlib,shutil,sys
ROOT=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('source_prepare',ROOT.parent/'pathak/prepare.py');source=importlib.util.module_from_spec(spec);spec.loader.exec_module(source)
source.ROOT=ROOT;source.CASES=['N4040_R12','N394FX_R20','N916KT_R22','N757AH_R41','N413AK_R49']
if __name__=='__main__':
 for name in ['final_results_40_cases.csv','source_record.json']:
  shutil.copy2(ROOT.parent/'pathak'/name,ROOT/name)
 source.main()
 if '--verify' not in sys.argv:
  p=ROOT/'manifest.json';m=json.loads(p.read_text());m['selection']='All five unused cases in the source constant-speed/height category. Declared before acoustic evaluation; no ranking or outcome selection.';m['role']='Separate recordings reserved for frozen algorithm evaluation. Source source-reference offsets and UTC labels are evaluation/declared time-prior data, not hand-supplied ridges.';p.write_text(json.dumps(m,indent=2)+'\n')
