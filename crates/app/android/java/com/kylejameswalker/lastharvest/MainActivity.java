package com.kylejameswalker.lastharvest;

import android.app.NativeActivity;
import android.app.Presentation;
import android.content.Context;
import android.media.AudioAttributes;
import android.graphics.PixelFormat;
import android.hardware.display.DisplayManager;
import android.os.Bundle;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.view.Display;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.WindowManager;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;

/**
 * The game's NativeActivity, with what the NDK can't reach: the vibrator, and a second screen, which Android shows
 * as a separate presentation display. The bottom screen draws into a SurfaceView on that display, and its touches go
 * to the game as bottom-screen touches.
 */
public class MainActivity extends NativeActivity implements DisplayManager.DisplayListener {
    static {
        // NativeActivity loads the library itself, but JNI only finds native methods in ones loaded here.
        System.loadLibrary("last_harvest");
    }

    /** The second screen's window and its touches, which android.rs takes. A null surface takes it away. */
    static native void secondSurface(Surface surface);

    /** A finger on the second screen: phase 0 down, 1 moved, 2 up; x and y from 0 to 1 across it. */
    static native void secondTouch(int phase, int id, float x, float y);

    /** The pack fetchPack asked for has arrived, or failed with this error. */
    static native void packReady(String error);

    private Presentation presentation;
    private DisplayManager displays;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        displays = (DisplayManager) getSystemService(Context.DISPLAY_SERVICE);
        displays.registerDisplayListener(this, null);
        showSecondScreen();
    }

    @Override
    protected void onDestroy() {
        displays.unregisterDisplayListener(this);
        hideSecondScreen();
        super.onDestroy();
    }

    @Override
    public void onDisplayAdded(int id) {
        showSecondScreen();
    }

    @Override
    public void onDisplayRemoved(int id) {
        if (presentation != null && presentation.getDisplay().getDisplayId() == id) {
            hideSecondScreen();
        }
    }

    @Override
    public void onDisplayChanged(int id) {}

    private void showSecondScreen() {
        if (presentation != null) {
            return;
        }
        Display[] found = displays.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION);
        if (found.length == 0) {
            return;
        }
        presentation = new Presentation(this, found[0]);
        SurfaceView view = new SurfaceView(presentation.getContext());
        view.getHolder().setFormat(PixelFormat.RGBA_8888);
        view.getHolder().addCallback(new SurfaceHolder.Callback() {
            @Override
            public void surfaceCreated(SurfaceHolder holder) {}

            @Override
            public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
                secondSurface(holder.getSurface());
            }

            @Override
            public void surfaceDestroyed(SurfaceHolder holder) {
                secondSurface(null);
            }
        });
        view.setOnTouchListener((v, e) -> {
            float w = Math.max(1, v.getWidth()), h = Math.max(1, v.getHeight());
            int action = e.getActionMasked();
            if (action == MotionEvent.ACTION_MOVE) {
                for (int i = 0; i < e.getPointerCount(); i++) {
                    secondTouch(1, e.getPointerId(i), e.getX(i) / w, e.getY(i) / h);
                }
                return true;
            }
            int phase = switch (action) {
                case MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> 0;
                case MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> 2;
                default -> -1;
            };
            if (phase >= 0) {
                int i = e.getActionIndex();
                secondTouch(phase, e.getPointerId(i), e.getX(i) / w, e.getY(i) / h);
            }
            return true;
        });
        presentation.setContentView(view);
        // Touches still reach it, but the gamepad's keys stay with the game's window.
        presentation.getWindow().addFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE);
        presentation.show();
    }

    private void hideSecondScreen() {
        if (presentation != null) {
            presentation.dismiss();
            presentation = null;
        }
    }

    /** Called from android.rs when the APK carries no game_data.pak: fetches url to path off the UI thread. */
    public void fetchPack(String url, String path) {
        new Thread(() -> {
            File out = new File(path), part = new File(path + ".part");
            String error = null;
            try {
                HttpURLConnection c = (HttpURLConnection) new URL(url).openConnection();
                c.setConnectTimeout(10000);
                c.setReadTimeout(30000);
                if (c.getResponseCode() != HttpURLConnection.HTTP_OK) {
                    throw new IOException("HTTP " + c.getResponseCode() + " for " + url);
                }
                try (InputStream in = c.getInputStream(); OutputStream to = new FileOutputStream(part)) {
                    byte[] buffer = new byte[65536];
                    for (int n; (n = in.read(buffer)) > 0; ) {
                        to.write(buffer, 0, n);
                    }
                }
                if (!part.renameTo(out)) {
                    throw new IOException("can't move " + part + " to " + out);
                }
            } catch (Exception e) {
                error = e.toString();
                part.delete();
            }
            packReady(error);
        }).start();
    }

    /** Called from android.rs while the game's rumble is on. */
    public void vibrate(int ms) {
        Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        if (v != null && v.hasVibrator()) {
            AudioAttributes game = new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).build();
            v.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE), game);
        }
    }

    /** Called from android.rs when the game's rumble stops. */
    public void stopVibrate() {
        Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        if (v != null) {
            v.cancel();
        }
    }
}
