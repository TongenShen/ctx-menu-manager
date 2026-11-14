// Windows 右键菜单管理器 —— 可执行入口。

// 始终以 Windows GUI 子系统链接，debug 构建也不再附带黑色控制台窗口。
// 代价：debug 下 println!/panic 输出没有地方显示，调试时改为写入日志或
// 用 IDE 调试器查看。需要临时看控制台输出时，注释掉这一行即可。
#![windows_subsystem = "windows"]

fn main() {
    ctx_menu_manager_lib::run()
}
