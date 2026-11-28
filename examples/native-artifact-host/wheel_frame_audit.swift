// Fixture-specific audit for the self-contained dark reported artifact. White
// authored content is NOT a failure in general. Inset excludes window chrome.
import AppKit
guard CommandLine.arguments.count == 2 else { fatalError("usage: wheel-frame-audit capture-directory") }
let dir=CommandLine.arguments[1]
let root=URL(fileURLWithPath:dir)
let timeline = (try? JSONDecoder().decode([String:Double].self,from:Data(contentsOf:root.appendingPathComponent("timeline.json")))) ?? [:]
let times = (try? JSONDecoder().decode([Double].self,from:Data(contentsOf:root.appendingPathComponent("frame-times.json")))) ?? []
var phaseCounts:[String:Int]=[:],whiteCounts:[String:Int]=[:]

let files=try! FileManager.default.contentsOfDirectory(atPath:dir).filter{$0.hasSuffix(".png")}.sorted()
var ranked:[(Int,String)]=[]
guard !files.isEmpty && files.count <= 2400 else { fputs("No valid bounded PNG capture to audit\n",stderr);exit(2) }
for (index,file) in files.enumerated() {
 let rep=NSBitmapImageRep(data:try! Data(contentsOf:URL(fileURLWithPath:"\(dir)/\(file)")))!
 guard rep.pixelsWide>60 && rep.pixelsHigh>130 && rep.samplesPerPixel>=3 else { fatalError("invalid capture pixels") }
 let p=rep.bitmapData!; var white=0
 for y in 100..<(rep.pixelsHigh-30) {for x in 30..<(rep.pixelsWide-30) {let i=y*rep.bytesPerRow+x*rep.samplesPerPixel;if p[i]>242 && p[i+1]>242 && p[i+2]>242 {white+=1}}}
 let relative = index < times.count ? times[index] - (timeline["injection_start"] ?? times[index]) : 0
 let phase = index >= times.count || timeline.isEmpty ? "unclassified" : (relative < 0 ? "quiet" : (times[index] <= (timeline["injection_end"] ?? 0) ? "motion" : "after"))
 phaseCounts[phase,default:0]+=1
 if white>0 {whiteCounts[phase,default:0]+=1}
 ranked.append((white,file));print(file,white,phase,relative)

}
ranked.sort{$0.0 > $1.0}
let selected = Array(ranked.prefix(8)).map{$0.1}+stride(from:0,to:files.count,by:max(1,files.count/8)).map{files[$0]}
let sheet=NSImage(size:NSSize(width:1000,height:840));sheet.lockFocus()
for (i,file) in selected.prefix(16).enumerated() {let img=NSImage(contentsOfFile:"\(dir)/\(file)")!; img.draw(in:NSRect(x:(i%4)*250,y:(3-i/4)*210,width:250,height:190)); (file as NSString).draw(at:NSPoint(x:(i%4)*250,y:(3-i/4)*210+190),withAttributes:[.foregroundColor:NSColor.red])}
sheet.unlockFocus();let rep=NSBitmapImageRep(data:sheet.tiffRepresentation!)!;try! rep.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:"\(dir)-contact.png"));print("MAX",ranked.first!)

print("PHASE_FRAMES",phaseCounts,"WHITE_FRAMES",whiteCounts)
