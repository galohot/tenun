// Only the ECharts parts Tenun uses, to keep the app and the export small.
import * as echarts from 'echarts/core';
import { BarChart, CustomChart, LineChart, MapChart, PieChart, SankeyChart, ScatterChart } from 'echarts/charts';
import { AriaComponent, GeoComponent, GraphicComponent, GridComponent, LegendComponent, MarkAreaComponent, MarkLineComponent, MarkPointComponent, TooltipComponent } from 'echarts/components';
import { LabelLayout } from 'echarts/features';
import { CanvasRenderer } from 'echarts/renderers';

echarts.use([
  BarChart,
  CustomChart,
  LineChart,
  MapChart,
  PieChart,
  SankeyChart,
  ScatterChart,
  AriaComponent,
  GeoComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TooltipComponent,
  LabelLayout,
  CanvasRenderer,
]);

export { echarts };
