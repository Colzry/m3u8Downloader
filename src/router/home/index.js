import Home from "@/views/Home/index.vue"
import DList from "@/router/home/dList.js";
import DCompleted from "@/router/home/dCompleted.js";
import Setting from "@/router/home/setting.js";

export default [
  {
    path: '/home',
    name: 'Home',
    component: Home,
    children: [
      ...DList,
      ...DCompleted,
      ...Setting
    ]
  }
];