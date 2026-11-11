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

## EN

The last wizard step is **Outputs**. With FreeCAD 1.1 installed (otherwise the step shows an install link):

1. Tick the parts to generate (the part set depends on the drone type).
2. Choose the target folder and press **Generate**.
3. Each part produces a parametric `.FCStd` (with a `Params` spreadsheet and a `param_schema_version` row) and a `.STEP` file. Parts are delivered whole.
4. All parts are gathered with `App::Link` into one assembly `.FCStd`, plus a single assembly `.STEP`. Mirrored copies of symmetric parts are Part objects, not links. Placement uses approximate positions derived from the parameters.
5. `BOM.md` and `BOM.csv`: part, quantity, material, computed mass (CAD solid volume x density; internal voids are not modelled).
6. `Montaj_Raporu.md`: part pairs with volume intersection, total mass and center of gravity. **Apply CAD mass to design** updates the project's structure mass only with your confirmation; the center of gravity is only reported.

To change parameters, edit the `Params` sheet in FreeCAD and recompute with `F9`.
