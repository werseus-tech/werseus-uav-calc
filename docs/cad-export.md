# WerSeuS - UAV Calc: Çıktılar (CAD Parçaları, Montaj, Malzeme Listesi)
# Outputs (CAD Parts, Assembly, Bill of Materials)

## TR

Sihirbazın son adımı **Çıktılar**'dır. FreeCAD 1.1 kuruluysa (aksi halde adımda kurulum bağlantısı gösterilir):

1. Üretmek istediğiniz parçaları onay kutularıyla seçin (parça seti drone türüne göre değişir).
2. Hedef klasörü seçin ve **Üret**'e basın.
3. Her parça için parametrik `.FCStd` (içinde `Params` elektronik tablosu ve `param_schema_version` satırı) ve `.STEP` dosyası üretilir. Parçalar bütün verilir.
4. Tüm parçalar `App::Link` ile tek bir montaj `.FCStd` dosyasında toplanır; ayrıca tek montaj `.STEP` üretilir. Sağ/sol simetrik parçalarda aynalanan kopya bağlantı değil, Part nesnesidir. Yerleşim parametrelerden türetilen yaklaşık konumlardır.
5. `BOM.md` ve `BOM.csv`: parça, adet, malzeme, hesaplanan kütle (CAD katı hacmi x yoğunluk; iç boşluk modellenmez).
6. `Montaj_Raporu.md`: hacim kesişimi olan parça çiftleri, toplam kütle ve ağırlık merkezi. **CAD kütlesini tasarıma aktar** düğmesi yalnızca onayınızla projedeki yapı kütlesini günceller; ağırlık merkezi yalnızca raporlanır.

Parametreleri değiştirmek için FreeCAD'de `Params` tablosunu düzenleyip `F9` ile yeniden hesaplayın.

### Hızlı altyapı (build123d, varsayılan)

Çıktılar adımında **CAD altyapısı** olarak **Hızlı (STEP/STL/DXF)** varsayılandır; **Düzenlenebilir FreeCAD ağacı** yukarıdaki FreeCAD yoludur. Hızlı altyapı aynı yerleşim (`Layout`), parça parametreleri ve iç yapıyla (kabuk, kaburgalar, kiriş kanalı, gövde boşluğu ve postalar, yuvalar) çalışır ve tüm parçaları tek bir Python sürecinde üretir:

- Her parça için `.step` ve `.stl`; kaburgalar, postalar ve düz plakalar için yan yana dizilmiş `.dxf` profilleri (onay kutularıyla seçilir).
- Tüm parçaların yerleştiği tek montaj `Montaj.step`; `BOM.md`, `BOM.csv` ve `Montaj_Raporu.md` FreeCAD yoluyla aynıdır.
- build123d kurulu bir Python gerekir (`pip install build123d`, Python 3.10-3.13). Python yolu **Ayarlar > CAD** bölümünden verilebilir; boşsa PATH ve `py` başlatıcısı taranır. Bulunamazsa adım kurulum komutunu ve FreeCAD'e geçiş düğmesini gösterir.
- Süre sınırları, iptal ve yarım dosya temizliği FreeCAD yoluyla aynıdır.

## EN

The last wizard step is **Outputs**. With FreeCAD 1.1 installed (otherwise the step shows an install link):

1. Tick the parts to generate (the part set depends on the drone type).
2. Choose the target folder and press **Generate**.
3. Each part produces a parametric `.FCStd` (with a `Params` spreadsheet and a `param_schema_version` row) and a `.STEP` file. Parts are delivered whole.
4. All parts are gathered with `App::Link` into one assembly `.FCStd`, plus a single assembly `.STEP`. Mirrored copies of symmetric parts are Part objects, not links. Placement uses approximate positions derived from the parameters.
5. `BOM.md` and `BOM.csv`: part, quantity, material, computed mass (CAD solid volume x density; internal voids are not modelled).
6. `Montaj_Raporu.md`: part pairs with volume intersection, total mass and center of gravity. **Apply CAD mass to design** updates the project's structure mass only with your confirmation; the center of gravity is only reported.

To change parameters, edit the `Params` sheet in FreeCAD and recompute with `F9`.

### Fast backend (build123d, default)

In the Outputs step the **CAD backend** defaults to **Fast (STEP/STL/DXF)**; **Editable FreeCAD tree** is the FreeCAD path above. The fast backend uses the same layout (`Layout`), part parameters and internal structure (skin shell, ribs, spar channel, fuselage void and formers, slots) and builds all parts in one Python process:

- `.step` and `.stl` per part; `.dxf` profiles laid side by side for ribs, formers and flat plates (selectable with checkboxes).
- One assembled `Montaj.step` with every part placed; `BOM.md`, `BOM.csv` and `Montaj_Raporu.md` are the same as with FreeCAD.
- Needs a Python with build123d (`pip install build123d`, Python 3.10-3.13). The interpreter can be set in **Settings > CAD**; if empty, PATH and the `py` launcher are searched. If none is found the step shows the install command and a switch to FreeCAD.
- Time limits, cancel and partial-file cleanup are the same as the FreeCAD path.
