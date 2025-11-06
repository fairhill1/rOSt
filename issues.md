Adding 2 debug lines makes the entire UI red



⏺ Update(userspace/window_manager/main.rs)
  ⎿  Updated userspace/window_manager/main.rs with 2 additions                   
       1140                            need_redraw.store(true, Ordering::SeqCst);
       1141                        }
       1142                        KernelToWM::CloseWindow { id } => {
       1143 +                          print_debug("WM:GOT_CLOSEWINDOW!");
       1144                            handle_close_window(id);
       1145 +                          print_debug("WM:WINDOW_CLOSED!");
       1146                            need_redraw.store(true, Ordering::SeqCst);
       1147                        }
       1148                        KernelToWM::SetFocus { id } => {

